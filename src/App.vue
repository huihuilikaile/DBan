<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  mode, track, toastMsg, todoHistory, todos, vaults, apps,
  activeAppCategoryId, initStore, initEvents, disposeEvents, saveApps, setMode, toast,
} from "./store";
import type { AppItem } from "./store";
import CapsuleBar from "./components/CapsuleBar.vue";
import PanelHeader from "./components/PanelHeader.vue";
import TodoList from "./components/TodoList.vue";
import VaultList from "./components/VaultList.vue";
import AppLauncher from "./components/AppLauncher.vue";
import HistoryList from "./components/HistoryList.vue";
import MiniPlayer from "./components/MiniPlayer.vue";

const activeTab = ref<"todo" | "vault" | "apps" | "history">("todo");
const tooltipText = ref("");
const tooltipVisible = ref(false);
const tooltipX = ref(0);
const tooltipY = ref(0);
const tooltipArrowX = ref(0);
const tooltipDirection = ref<"up" | "down">("down");
const tooltipEl = ref<HTMLElement | null>(null);
let unlistenDragDrop: (() => void) | undefined;
let tooltipTarget: HTMLElement | null = null;
let tooltipTimer: ReturnType<typeof setTimeout> | undefined;

function preventContextMenu(e: MouseEvent) {
  e.preventDefault();
}

function tooltipElement(target: EventTarget | null) {
  return target instanceof Element ? target.closest<HTMLElement>("[data-tooltip]") : null;
}

function hideTooltip(target?: HTMLElement | null) {
  if (target && tooltipTarget !== target) return;
  clearTimeout(tooltipTimer);
  tooltipTimer = undefined;
  tooltipTarget = null;
  tooltipVisible.value = false;
  tooltipText.value = "";
}

function scheduleTooltip(target: HTMLElement) {
  const text = target.dataset.tooltip?.trim();
  if (!text || tooltipTarget === target) return;
  hideTooltip();
  tooltipTarget = target;
  tooltipTimer = setTimeout(async () => {
    if (tooltipTarget !== target || !target.isConnected) return;
    tooltipText.value = text;
    await nextTick();
    const tooltip = tooltipEl.value;
    if (!tooltip || tooltipTarget !== target) return;

    const margin = 8;
    const gap = 8;
    const rect = target.getBoundingClientRect();
    const width = tooltip.offsetWidth;
    const height = tooltip.offsetHeight;
    const centeredX = rect.left + rect.width / 2 - width / 2;
    const maxX = Math.max(margin, window.innerWidth - width - margin);
    const x = Math.max(margin, Math.min(centeredX, maxX));
    const roomAbove = rect.top - margin;
    const roomBelow = window.innerHeight - rect.bottom - margin;
    const showAbove = roomAbove >= height + gap && (roomBelow < height + gap || rect.top > window.innerHeight / 2);
    const y = showAbove ? rect.top - height - gap : rect.bottom + gap;

    tooltipX.value = x;
    tooltipY.value = Math.max(margin, Math.min(y, window.innerHeight - height - margin));
    tooltipArrowX.value = Math.max(10, Math.min(rect.left + rect.width / 2 - x, width - 10));
    tooltipDirection.value = showAbove ? "up" : "down";
    tooltipVisible.value = true;
  }, 480);
}

function handleTooltipOver(event: MouseEvent) {
  const target = tooltipElement(event.target);
  if (!target || tooltipElement(event.relatedTarget) === target) return;
  scheduleTooltip(target);
}

function handleTooltipOut(event: MouseEvent) {
  const target = tooltipElement(event.target);
  if (!target || tooltipElement(event.relatedTarget) === target) return;
  hideTooltip(target);
}

function handleTooltipFocus(event: FocusEvent) {
  const target = tooltipElement(event.target);
  if (target) scheduleTooltip(target);
}

function handleTooltipBlur(event: FocusEvent) {
  hideTooltip(tooltipElement(event.target));
}

function handleKeydown(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
  if (document.querySelector(".modal-backdrop, .app-context-menu")) return;
  if (e.key === "Escape" && mode.value === "panel") setMode("hidden");
}

onMounted(async () => {
  try {
    await initStore();
  } catch (e) {
    toast(`读取本地数据失败：${e}`);
  }
  try {
    await initEvents();
    mode.value = await invoke<"panel" | "capsule" | "hidden">("get_mode_command");
  } catch (e) {
    toast(`初始化系统能力失败：${e}`);
  }

  // 图标迁移：用高清提取（256px）刷新已有应用条目的图标
  const missingIconPaths = apps.value.filter((app) => !app.icon).map((app) => app.path);
  if (missingIconPaths.length) {
    invoke<AppItem[]>("add_apps", { paths: missingIconPaths }).then((added) => {
      let changed = false;
      for (const a of added) {
        const cur = apps.value.find((x) => x.path === a.path);
        if (cur && a.icon && cur.icon !== a.icon) {
          cur.icon = a.icon;
          changed = true;
        }
      }
      if (changed) saveApps();
    }).catch((e) => toast(`刷新应用图标失败：${e}`));
  }
  // 禁用 webview 右键菜单
  window.addEventListener("contextmenu", preventContextMenu);
  window.addEventListener("mouseover", handleTooltipOver);
  window.addEventListener("mouseout", handleTooltipOut);
  window.addEventListener("focusin", handleTooltipFocus);
  window.addEventListener("focusout", handleTooltipBlur);

  // Esc 隐藏面板（全局 Alt+D / 热角 / 托盘在 Rust 侧处理）
  window.addEventListener("keydown", handleKeydown);

  // 拖拽 .exe / .lnk 到面板 = 添加快捷启动
  unlistenDragDrop = await getCurrentWebview().onDragDropEvent((ev) => {
    if (ev.payload.type !== "drop") return;
    const paths = ev.payload.paths.filter((p) => /\.(exe|lnk)$/i.test(p));
    if (!paths.length) return;
    invoke<AppItem[]>("add_apps", { paths }).then((added) => {
      let n = 0;
      for (const a of added) {
        if (!apps.value.some((x) => x.path === a.path)) {
          apps.value.push({
            ...a,
            categoryId: activeAppCategoryId.value === "all" ? undefined : activeAppCategoryId.value,
          });
          n++;
        }
      }
      if (n) {
        saveApps();
        activeTab.value = "apps";
        toast(`已添加 ${n} 个应用`);
      }
    }).catch((e) => toast(`添加应用失败：${e}`));
  });
});

onBeforeUnmount(() => {
  window.removeEventListener("contextmenu", preventContextMenu);
  window.removeEventListener("mouseover", handleTooltipOver);
  window.removeEventListener("mouseout", handleTooltipOut);
  window.removeEventListener("focusin", handleTooltipFocus);
  window.removeEventListener("focusout", handleTooltipBlur);
  window.removeEventListener("keydown", handleKeydown);
  hideTooltip();
  unlistenDragDrop?.();
  disposeEvents();
});
</script>

<template>
  <CapsuleBar v-if="mode === 'capsule'" />
  <div v-else-if="mode === 'panel'" class="panel">
    <PanelHeader :date-text="new Date().toLocaleDateString('zh-CN', { month: 'long', day: 'numeric', weekday: 'short' })" />

    <nav class="tabs">
      <button class="tab" :class="{ active: activeTab === 'todo' }" @click="activeTab = 'todo'">
        待办 <em>{{ todos.length }}</em>
      </button>
      <button class="tab" :class="{ active: activeTab === 'vault' }" @click="activeTab = 'vault'">
        密码 <em>{{ vaults.length }}</em>
      </button>
      <button class="tab" :class="{ active: activeTab === 'apps' }" @click="activeTab = 'apps'">
        应用 <em>{{ apps.length }}</em>
      </button>
      <button class="tab" :class="{ active: activeTab === 'history' }" @click="activeTab = 'history'">
        历史 <em>{{ todoHistory.length }}</em>
      </button>
    </nav>

    <TodoList v-show="activeTab === 'todo'" />
    <VaultList v-show="activeTab === 'vault'" />
    <AppLauncher v-show="activeTab === 'apps'" />
    <HistoryList v-show="activeTab === 'history'" />

    <MiniPlayer :track="track" />

    <div class="toast" :class="{ show: toastMsg }">{{ toastMsg }}</div>
  </div>
  <!-- hidden：窗口本身已隐藏 -->
  <div
    v-if="tooltipText"
    ref="tooltipEl"
    class="app-tooltip"
    :class="[`arrow-${tooltipDirection}`, { show: tooltipVisible }]"
    :style="{
      left: `${tooltipX}px`,
      top: `${tooltipY}px`,
      '--tooltip-arrow-x': `${tooltipArrowX}px`,
    }"
    role="tooltip"
  >
    {{ tooltipText }}
  </div>
</template>
