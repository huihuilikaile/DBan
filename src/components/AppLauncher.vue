<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
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

type ContextMenu =
  | { type: "area"; x: number; y: number }
  | { type: "app"; appId: string; x: number; y: number }
  | { type: "category"; categoryId: string; x: number; y: number }
  | null;

type CategoryDialog = { type: "create" | "rename"; categoryId?: string } | null;

const lastLaunchAt = new Map<string, number>();
const contextMenu = ref<ContextMenu>(null);
const contextMenuEl = ref<HTMLElement | null>(null);
const categoryDialog = ref<CategoryDialog>(null);
const categoryName = ref("");
const categoryInput = ref<HTMLInputElement | null>(null);
const pendingDelete = ref<AppCategory | null>(null);

const visibleApps = computed(() => {
  if (activeAppCategoryId.value === "all") return apps.value;
  return apps.value.filter((app) => app.categoryId === activeAppCategoryId.value);
});
const contextAppId = computed(() => contextMenu.value?.type === "app" ? contextMenu.value.appId : "");
const contextAppCategoryId = computed(() => (
  apps.value.find((app) => app.id === contextAppId.value)?.categoryId
));

function categoryCount(categoryId: string) {
  if (categoryId === "all") return apps.value.length;
  return apps.value.filter((app) => app.categoryId === categoryId).length;
}

async function merge(added: AppItem[]) {
  let n = 0;
  for (const app of added) {
    if (!apps.value.some((current) => current.path === app.path)) {
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
  } else {
    toast("应用已存在");
  }
}

async function browse() {
  const file = await open({
    multiple: false,
    filters: [{ name: "应用程序", extensions: ["exe", "lnk"] }],
  });
  if (!file || typeof file !== "string") return;
  const added = await invoke<AppItem[]>("add_apps", { paths: [file] });
  await merge(added);
}

async function launch(app: AppItem) {
  const now = Date.now();
  if (now - (lastLaunchAt.get(app.id) ?? 0) < 700) return;
  lastLaunchAt.set(app.id, now);
  try {
    await invoke("launch_app", { path: app.path });
    toast(`已启动「${app.name}」`);
  } catch (e) {
    toast(`启动失败：${e}`);
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

onMounted(() => {
  window.addEventListener("click", closeContextMenu);
  window.addEventListener("keydown", handleKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener("click", closeContextMenu);
  window.removeEventListener("keydown", handleKeydown);
});
</script>

<template>
  <section class="pane apps-pane" @contextmenu="openAreaMenu">
    <div class="apps-scroll">
      <div v-if="visibleApps.length" class="apps-grid">
        <div
          v-for="app in visibleApps"
          :key="app.id"
          class="app"
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
        </div>
      </div>
      <div v-else class="apps-empty">此分类暂无应用</div>

      <button class="addtile" @click="browse">＋ 添加应用（或拖入 .exe / .lnk）</button>
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
