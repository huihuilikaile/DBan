<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  activeAppCategoryId,
  appCategories,
  apps,
  saveActiveAppCategory,
  saveAppCategories,
  saveApps,
  toast,
} from "../store";
import type { AppCategory, AppItem } from "../store";

const props = defineProps<{ active: boolean }>();

type ContextMenu =
  | { type: "area"; x: number; y: number }
  | { type: "app"; appId: string; x: number; y: number }
  | { type: "category"; categoryId: string; x: number; y: number }
  | null;
type CategoryDialog = { type: "create" | "rename"; categoryId?: string } | null;
type DragPosition = { x: number; y: number };
type DragPathInfo = { isFile: boolean; isDirectory: boolean; canAddApp: boolean };

const lastLaunchAt = new Map<string, number>();
const contextMenu = ref<ContextMenu>(null);
const contextMenuEl = ref<HTMLElement | null>(null);
const categoryDialog = ref<CategoryDialog>(null);
const categoryName = ref("");
const categoryInput = ref<HTMLInputElement | null>(null);
const pendingDelete = ref<AppCategory | null>(null);
const appsPane = ref<HTMLElement | null>(null);
const appsScroll = ref<HTMLElement | null>(null);

const dragActive = ref(false);
const dragPaths = ref<string[]>([]);
const dragPathInfo = ref<DragPathInfo | null>(null);
const dragInvalidMessage = ref("");
const dragTargetAppId = ref("");
const dragOverBlank = ref(false);
const dragPreview = ref<AppItem | null>(null);
const operationBusy = ref(false);
const openingAppId = ref("");
let unlistenDragDrop: (() => void) | undefined;
let dragGeneration = 0;
let autoScrollTimer: ReturnType<typeof setTimeout> | undefined;
let inspectionPromise: Promise<DragPathInfo | null> | null = null;
let previewPromise: Promise<AppItem | null> | null = null;

const visibleApps = computed(() => {
  if (activeAppCategoryId.value === "all") return apps.value;
  return apps.value.filter((app) => app.categoryId === activeAppCategoryId.value);
});
const contextAppId = computed(() => contextMenu.value?.type === "app" ? contextMenu.value.appId : "");
const contextAppCategoryId = computed(() => (
  apps.value.find((app) => app.id === contextAppId.value)?.categoryId
));
const draggedPath = computed(() => dragPaths.value.length === 1 ? dragPaths.value[0] : "");
const draggedName = computed(() => fileNameFromPath(draggedPath.value));
const optimisticCanAdd = computed(() => /\.(exe|lnk)$/i.test(draggedPath.value));
const canAddDraggedApp = computed(() => dragPathInfo.value?.canAddApp ?? optimisticCanAdd.value);
const duplicateApp = computed(() => {
  const key = normalizeWindowsPath(draggedPath.value);
  return key ? apps.value.find((app) => normalizeWindowsPath(app.path) === key) : undefined;
});
const showAddPreview = computed(() => (
  dragActive.value
  && !dragInvalidMessage.value
  && !operationBusy.value
  && dragOverBlank.value
  && canAddDraggedApp.value
));
const dragGuideText = computed(() => {
  if (!dragActive.value) return "";
  if (dragInvalidMessage.value) return dragInvalidMessage.value;
  if (operationBusy.value) return "正在处理，请稍候";
  if (dragOverBlank.value && !canAddDraggedApp.value) return "拖到应用上以打开";
  return "";
});

function categoryCount(categoryId: string) {
  if (categoryId === "all") return apps.value.length;
  return apps.value.filter((app) => app.categoryId === categoryId).length;
}

function normalizeWindowsPath(path: string) {
  let value = path.trim().replaceAll("/", "\\");
  if (/^\\\\\?\\UNC\\/i.test(value)) value = `\\\\${value.slice(8)}`;
  else value = value.replace(/^\\\\\?\\/i, "");
  if (!/^[a-z]:\\$/i.test(value) && !/^\\\\[^\\]+\\[^\\]+\\?$/i.test(value)) {
    value = value.replace(/\\+$/, "");
  }
  return value.normalize("NFKC").toLocaleLowerCase("en-US");
}

function fileNameFromPath(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? "文件";
}

function previewNameFromPath(path: string) {
  return fileNameFromPath(path).replace(/\.(exe|lnk)$/i, "") || "应用";
}

function categoryLabel(app: AppItem) {
  if (!app.categoryId) return "未分类";
  return appCategories.value.find((category) => category.id === app.categoryId)?.name ?? "未知分类";
}

function clearAutoScroll() {
  clearTimeout(autoScrollTimer);
  autoScrollTimer = undefined;
}

function resetDragState() {
  dragGeneration++;
  clearAutoScroll();
  dragActive.value = false;
  dragPaths.value = [];
  dragPathInfo.value = null;
  dragInvalidMessage.value = "";
  dragTargetAppId.value = "";
  dragOverBlank.value = false;
  dragPreview.value = null;
  inspectionPromise = null;
  previewPromise = null;
}

function scheduleAutoScroll() {
  clearAutoScroll();
  if (!showAddPreview.value) return;
  const element = appsScroll.value;
  if (!element || element.scrollHeight <= element.clientHeight + element.scrollTop + 2) return;
  autoScrollTimer = setTimeout(() => {
    if (!showAddPreview.value) return;
    element.scrollTo({ top: element.scrollHeight, behavior: "smooth" });
  }, 400);
}

function updateDropTarget(position: DragPosition) {
  clearAutoScroll();
  dragTargetAppId.value = "";
  dragOverBlank.value = false;
  if (!props.active || !dragActive.value || categoryDialog.value || pendingDelete.value) return;
  if (operationBusy.value) return;

  const scale = window.devicePixelRatio || 1;
  const target = document.elementFromPoint(position.x / scale, position.y / scale);
  const pane = appsPane.value;
  const scroll = appsScroll.value;
  if (!(target instanceof Element) || !pane?.contains(target)) return;

  const app = target.closest<HTMLElement>(".app[data-app-id]");
  if (app?.dataset.appId && !dragInvalidMessage.value) {
    dragTargetAppId.value = app.dataset.appId;
    return;
  }

  if (scroll?.contains(target) && !target.closest(".addtile")) {
    dragOverBlank.value = true;
    scheduleAutoScroll();
  }
}

function beginDrag(paths: string[], position: DragPosition) {
  resetDragState();
  if (!props.active || categoryDialog.value || pendingDelete.value) return;
  closeContextMenu();
  dragActive.value = true;
  dragPaths.value = paths;
  const generation = dragGeneration;

  if (paths.length !== 1) {
    dragInvalidMessage.value = "每次只能拖入一个文件";
    updateDropTarget(position);
    return;
  }

  const path = paths[0];
  if (optimisticCanAdd.value) {
    dragPreview.value = {
      id: "drag-preview",
      name: previewNameFromPath(path),
      path,
      icon: duplicateApp.value?.icon ?? "",
    };
    previewPromise = invoke<AppItem[]>("add_apps", { paths: [path] })
      .then((items) => items[0] ?? null)
      .catch(() => null);
    void previewPromise.then((preview) => {
      if (generation === dragGeneration && preview) dragPreview.value = preview;
    });
  }

  inspectionPromise = invoke<DragPathInfo>("inspect_drag_path", { path })
    .then((info) => {
      if (generation === dragGeneration) {
        dragPathInfo.value = info;
        if (info.isDirectory) dragInvalidMessage.value = "暂不支持打开文件夹";
        else if (!info.isFile) dragInvalidMessage.value = "文件不存在或不可访问";
        if (dragInvalidMessage.value) {
          dragTargetAppId.value = "";
          dragOverBlank.value = false;
          clearAutoScroll();
        } else {
          scheduleAutoScroll();
        }
      }
      return info;
    })
    .catch(() => {
      if (generation === dragGeneration) dragInvalidMessage.value = "文件不存在或不可访问";
      return null;
    });
  updateDropTarget(position);
}

async function handleDrop(position: DragPosition) {
  if (!props.active || categoryDialog.value || pendingDelete.value || !dragActive.value) {
    resetDragState();
    return;
  }
  updateDropTarget(position);

  if (dragPaths.value.length !== 1) {
    toast("每次只能拖入一个文件");
    resetDragState();
    return;
  }
  if (operationBusy.value) {
    toast("正在处理，请稍候");
    resetDragState();
    return;
  }

  const path = draggedPath.value;
  const targetAppId = dragTargetAppId.value;
  const overBlank = dragOverBlank.value;
  const existing = duplicateApp.value;
  const pendingInspection = inspectionPromise;
  const pendingPreview = previewPromise;
  const dropGeneration = dragGeneration;
  operationBusy.value = true;

  const info = dragPathInfo.value ?? await pendingInspection;
  if (dropGeneration === dragGeneration) resetDragState();
  if (!info?.isFile) {
    toast(info?.isDirectory ? "暂不支持打开文件夹" : "文件不存在或不可访问");
    operationBusy.value = false;
    return;
  }

  const targetApp = apps.value.find((app) => app.id === targetAppId);
  if (targetApp) {
    await openDraggedFile(targetApp, path);
    return;
  }

  if (overBlank && info.canAddApp) {
    await addDraggedApp(path, existing, pendingPreview);
    return;
  }
  if (overBlank) toast("请选择一个应用打开此文件");
  operationBusy.value = false;
}

async function openDraggedFile(app: AppItem, path: string) {
  operationBusy.value = true;
  openingAppId.value = app.id;
  const startedAt = performance.now();
  try {
    await invoke("launch_app", { path: app.path, filePath: path });
    const remaining = 450 - (performance.now() - startedAt);
    if (remaining > 0) await new Promise((resolve) => setTimeout(resolve, remaining));
  } catch (e) {
    const remaining = 450 - (performance.now() - startedAt);
    if (remaining > 0) await new Promise((resolve) => setTimeout(resolve, remaining));
    toast(`打开失败：${e}`);
  } finally {
    openingAppId.value = "";
    operationBusy.value = false;
  }
}

async function addDraggedApp(
  path: string,
  existing: AppItem | undefined,
  pendingPreview: Promise<AppItem | null> | null,
) {
  operationBusy.value = true;
  try {
    if (existing) {
      toast(`应用已存在于「${categoryLabel(existing)}」`);
      return;
    }
    const preview = await pendingPreview ?? (await invoke<AppItem[]>("add_apps", { paths: [path] }))[0];
    if (!preview) throw new Error("无法读取应用信息");
    apps.value.push({
      ...preview,
      categoryId: activeAppCategoryId.value === "all" ? undefined : activeAppCategoryId.value,
    });
    await saveApps();
    toast(`已添加「${preview.name}」`);
  } catch (e) {
    toast(`添加应用失败：${e}`);
  } finally {
    operationBusy.value = false;
  }
}

async function merge(added: AppItem[]) {
  let n = 0;
  let existing: AppItem | undefined;
  for (const app of added) {
    existing = apps.value.find((current) => normalizeWindowsPath(current.path) === normalizeWindowsPath(app.path));
    if (!existing) {
      apps.value.push({
        ...app,
        categoryId: activeAppCategoryId.value === "all" ? undefined : activeAppCategoryId.value,
      });
      n++;
    }
  }
  if (n) {
    await saveApps();
    toast(`已添加 ${n} 个应用`);
  } else if (existing) {
    toast(`应用已存在于「${categoryLabel(existing)}」`);
  } else {
    toast("无法读取应用信息");
  }
}

async function browse() {
  if (operationBusy.value) {
    toast("正在处理，请稍候");
    return;
  }
  operationBusy.value = true;
  try {
    const file = await open({
      multiple: false,
      filters: [{ name: "应用程序", extensions: ["exe", "lnk"] }],
    });
    if (!file || typeof file !== "string") return;
    const added = await invoke<AppItem[]>("add_apps", { paths: [file] });
    await merge(added);
  } catch (e) {
    toast(`添加应用失败：${e}`);
  } finally {
    operationBusy.value = false;
  }
}

async function launch(app: AppItem) {
  const now = Date.now();
  if (now - (lastLaunchAt.get(app.id) ?? 0) < 700) return;
  if (operationBusy.value) {
    toast("正在处理，请稍候");
    return;
  }
  lastLaunchAt.set(app.id, now);
  operationBusy.value = true;
  try {
    await invoke("launch_app", { path: app.path, filePath: null });
    toast(`已启动「${app.name}」`);
  } catch (e) {
    toast(`启动失败：${e}`);
  } finally {
    operationBusy.value = false;
  }
}

async function removeApp(id: string) {
  apps.value = apps.value.filter((app) => app.id !== id);
  await saveApps();
}

async function selectCategory(id: string) {
  activeAppCategoryId.value = id;
  await saveActiveAppCategory();
  closeContextMenu();
}

function normalizedCategoryName() {
  return categoryName.value.trim();
}

function categoryNameExists(name: string, ignoredId?: string) {
  const normalized = name.toLocaleLowerCase();
  return appCategories.value.some(
    (category) => category.id !== ignoredId && category.name.toLocaleLowerCase() === normalized,
  );
}

function openCreateDialog() {
  closeContextMenu();
  categoryDialog.value = { type: "create" };
  categoryName.value = "";
  focusCategoryInput();
}

function openRenameDialog(categoryId: string) {
  const category = appCategories.value.find((item) => item.id === categoryId);
  if (!category) return;
  closeContextMenu();
  categoryDialog.value = { type: "rename", categoryId };
  categoryName.value = category.name;
  focusCategoryInput();
}

function focusCategoryInput() {
  nextTick(() => {
    categoryInput.value?.focus();
    categoryInput.value?.select();
  });
}

function closeCategoryDialog() {
  categoryDialog.value = null;
  categoryName.value = "";
}

async function submitCategoryDialog() {
  const dialog = categoryDialog.value;
  const name = normalizedCategoryName();
  if (!dialog) return;
  if (!name) {
    toast("分类名称不能为空");
    focusCategoryInput();
    return;
  }
  if (categoryNameExists(name, dialog.categoryId)) {
    toast("分类名称已存在");
    focusCategoryInput();
    return;
  }

  if (dialog.type === "create") {
    const category = { id: crypto.randomUUID(), name };
    appCategories.value.push(category);
    activeAppCategoryId.value = category.id;
    await Promise.all([saveAppCategories(), saveActiveAppCategory()]);
    toast(`已新建分类「${name}」`);
  } else {
    const category = appCategories.value.find((item) => item.id === dialog.categoryId);
    if (!category) return;
    category.name = name;
    await saveAppCategories();
    toast(`已重命名为「${name}」`);
  }
  closeCategoryDialog();
}

function requestDeleteCategory(categoryId: string) {
  const category = appCategories.value.find((item) => item.id === categoryId);
  if (!category) return;
  closeContextMenu();
  pendingDelete.value = category;
}

function cancelDeleteCategory() {
  pendingDelete.value = null;
}

async function confirmDeleteCategory() {
  const category = pendingDelete.value;
  if (!category) return;
  for (const app of apps.value) {
    if (app.categoryId === category.id) app.categoryId = undefined;
  }
  appCategories.value = appCategories.value.filter((item) => item.id !== category.id);
  if (activeAppCategoryId.value === category.id) activeAppCategoryId.value = "all";
  await Promise.all([saveApps(), saveAppCategories(), saveActiveAppCategory()]);
  pendingDelete.value = null;
  toast(`已删除分类「${category.name}」`);
}

async function moveApp(appId: string, categoryId?: string) {
  const app = apps.value.find((item) => item.id === appId);
  if (!app) return;
  app.categoryId = categoryId;
  await saveApps();
  closeContextMenu();
  const category = appCategories.value.find((item) => item.id === categoryId);
  toast(category ? `已移到「${category.name}」` : "已移到未分类");
}

function openContextMenu(event: MouseEvent, menu: Exclude<ContextMenu, null>) {
  if (dragActive.value) return;
  event.preventDefault();
  event.stopPropagation();
  contextMenu.value = menu;
  nextTick(() => {
    const element = contextMenuEl.value;
    const current = contextMenu.value;
    if (!element || !current) return;
    const margin = 8;
    current.x = Math.max(margin, Math.min(current.x, window.innerWidth - element.offsetWidth - margin));
    current.y = Math.max(margin, Math.min(current.y, window.innerHeight - element.offsetHeight - margin));
  });
}

function openAreaMenu(event: MouseEvent) {
  openContextMenu(event, { type: "area", x: event.clientX, y: event.clientY });
}

function openAppMenu(event: MouseEvent, appId: string) {
  openContextMenu(event, { type: "app", appId, x: event.clientX, y: event.clientY });
}

function openCategoryMenu(event: MouseEvent, categoryId: string) {
  openContextMenu(event, { type: "category", categoryId, x: event.clientX, y: event.clientY });
}

function closeContextMenu() {
  contextMenu.value = null;
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  closeContextMenu();
  closeCategoryDialog();
  cancelDeleteCategory();
}

watch(() => props.active, (active) => {
  if (!active) resetDragState();
});
watch([categoryDialog, pendingDelete], ([dialog, pending]) => {
  if (dialog || pending) resetDragState();
});

onMounted(async () => {
  window.addEventListener("click", closeContextMenu);
  window.addEventListener("keydown", handleKeydown);
  unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter") beginDrag(payload.paths, payload.position);
    else if (payload.type === "over") updateDropTarget(payload.position);
    else if (payload.type === "drop") void handleDrop(payload.position);
    else resetDragState();
  });
});

onBeforeUnmount(() => {
  window.removeEventListener("click", closeContextMenu);
  window.removeEventListener("keydown", handleKeydown);
  unlistenDragDrop?.();
  resetDragState();
});
</script>

<template>
  <section ref="appsPane" class="pane apps-pane" :class="{ 'external-drag-active': dragActive }" @contextmenu="openAreaMenu">
    <div ref="appsScroll" class="apps-scroll">
      <div
        v-if="visibleApps.length || showAddPreview"
        class="apps-grid"
        :class="{ 'preview-only': !visibleApps.length && showAddPreview }"
      >
        <div
          v-for="app in visibleApps"
          :key="app.id"
          class="app"
          :class="{
            'drop-open-target': dragTargetAppId === app.id,
            'drop-opening': openingAppId === app.id,
          }"
          :data-app-id="app.id"
          role="button"
          tabindex="0"
          :aria-label="`启动 ${app.name}`"
          @click="launch(app)"
          @keydown.enter.prevent="launch(app)"
          @keydown.space.prevent="launch(app)"
          @contextmenu.prevent.stop="openAppMenu($event, app.id)"
        >
          <span class="ai">
            <img v-if="app.icon" :src="app.icon" alt="" />
            <template v-else>{{ app.name[0] }}</template>
          </span>
          <span class="al">{{ app.name }}</span>
          <button class="rm" data-tooltip="移除应用" aria-label="移除应用" @click.stop="removeApp(app.id)">✕</button>
          <span v-if="dragTargetAppId === app.id" class="app-drop-message" aria-hidden="true">
            <small>使用</small><strong>{{ app.name }}</strong><small>打开</small>
          </span>
          <span v-else-if="openingAppId === app.id" class="app-drop-message opening" aria-live="polite">
            <small>正在使用</small><strong>{{ app.name }}</strong><small>打开</small>
          </span>
        </div>

        <div
          v-if="showAddPreview"
          class="app app-drop-preview"
          :class="{ duplicate: duplicateApp }"
          aria-live="polite"
        >
          <span class="ai">
            <img v-if="dragPreview?.icon" :src="dragPreview.icon" alt="" />
            <template v-else>{{ (dragPreview?.name || draggedName)[0] }}</template>
          </span>
          <span class="al">{{ dragPreview?.name || draggedName }}</span>
          <span class="app-preview-status">{{ duplicateApp ? "应用已存在" : "松手添加" }}</span>
        </div>
      </div>
      <div v-else class="apps-empty">此分类暂无应用</div>

      <button class="addtile" :disabled="operationBusy" @click="browse">＋ 添加应用（或拖入 .exe / .lnk）</button>
    </div>

    <div v-if="dragGuideText" class="app-drag-guide" aria-live="polite">
      {{ dragGuideText }}
    </div>

    <nav class="app-categories" aria-label="应用分类">
      <button
        class="category-tab"
        :class="{ active: activeAppCategoryId === 'all' }"
        @click="selectCategory('all')"
      >
        全部 <em>{{ categoryCount("all") }}</em>
      </button>
      <button
        v-for="category in appCategories"
        :key="category.id"
        class="category-tab"
        :class="{ active: activeAppCategoryId === category.id }"
        :data-tooltip="category.name"
        @click="selectCategory(category.id)"
        @contextmenu.prevent.stop="openCategoryMenu($event, category.id)"
      >
        <span>{{ category.name }}</span><em>{{ categoryCount(category.id) }}</em>
      </button>
    </nav>

    <div
      v-if="contextMenu"
      ref="contextMenuEl"
      class="app-context-menu"
      :style="{ left: `${contextMenu.x}px`, top: `${contextMenu.y}px` }"
      @click.stop
      @contextmenu.prevent.stop
    >
      <template v-if="contextMenu.type === 'area'">
        <button @click="openCreateDialog">
          <span class="context-icon">＋</span>新建分类
        </button>
      </template>

      <template v-else-if="contextMenu.type === 'app'">
        <div class="context-title">移动到</div>
        <button @click="moveApp(contextAppId)">
          <span class="context-icon">{{ contextAppCategoryId ? "" : "✓" }}</span>
          未分类
        </button>
        <button
          v-for="category in appCategories"
          :key="category.id"
          @click="moveApp(contextAppId, category.id)"
        >
          <span class="context-icon">{{ contextAppCategoryId === category.id ? "✓" : "" }}</span>
          {{ category.name }}
        </button>
      </template>

      <template v-else>
        <button @click="openRenameDialog(contextMenu.categoryId)">
          <span class="context-icon">✎</span>重命名
        </button>
        <button class="danger" @click="requestDeleteCategory(contextMenu.categoryId)">
          <span class="context-icon">×</span>删除分类
        </button>
      </template>
    </div>

    <div v-if="categoryDialog" class="modal-backdrop" role="presentation" @click.self="closeCategoryDialog">
      <form class="confirm-dialog category-dialog" @submit.prevent="submitCategoryDialog">
        <h2>{{ categoryDialog.type === "create" ? "新建应用分类" : "重命名分类" }}</h2>
        <input ref="categoryInput" v-model="categoryName" maxlength="18" placeholder="分类名称" />
        <div class="confirm-actions">
          <button type="button" @click="closeCategoryDialog">取消</button>
          <button class="primary" type="submit">确认</button>
        </div>
      </form>
    </div>

    <div v-if="pendingDelete" class="modal-backdrop" role="presentation" @click.self="cancelDeleteCategory">
      <section class="confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-category-title">
        <div class="confirm-icon" aria-hidden="true">
          <svg class="icon" viewBox="0 0 24 24"><path d="M12 9v4M12 17h.01M10.3 3.7 2.6 17a2 2 0 0 0 1.7 3h15.4a2 2 0 0 0 1.7-3L13.7 3.7a2 2 0 0 0-3.4 0z" /></svg>
        </div>
        <h2 id="delete-category-title">删除分类？</h2>
        <p>将删除分类「{{ pendingDelete.name }}」。分类内的应用不会被删除，它们会变为未分类应用。</p>
        <div class="confirm-actions">
          <button @click="cancelDeleteCategory">取消</button>
          <button class="danger" @click="confirmDeleteCategory">确认删除</button>
        </div>
      </section>
    </div>
  </section>
</template>
