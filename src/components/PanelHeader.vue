<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  autostart,
  DEFAULT_TOP_TRIGGER_DWELL_MS,
  DEFAULT_TOP_TRIGGER_WIDTH,
  globalShortcutEnabled,
  pinned,
  theme,
  topTriggerDwellMs,
  topTriggerWidth,
  setAutostart,
  setMode,
  saveGlobalShortcutEnabled,
  savePinned,
  saveTopTriggerSettings,
  saveTheme,
  toast,
} from "../store";

defineProps<{ dateText: string }>();

const menuOpen = ref(false);

async function togglePin() {
  const previous = pinned.value;
  pinned.value = !previous;
  try {
    await savePinned();
    toast(pinned.value ? "已置顶：面板始终在其他窗口之上" : "已取消置顶：面板不再强制置顶");
  } catch (e) {
    pinned.value = previous;
    toast(`置顶设置失败：${e}`);
  }
}

async function toggleTheme() {
  const previous = theme.value;
  theme.value = previous === "dark" ? "light" : "dark";
  try {
    await saveTheme();
  } catch (e) {
    theme.value = previous;
    toast(`主题保存失败：${e}`);
  }
}

async function toggleAutostart() {
  const enabled = await setAutostart(!autostart.value);
  if (enabled !== null) toast(enabled ? "已开启开机自启" : "已关闭开机自启");
}

async function toggleGlobalShortcut() {
  globalShortcutEnabled.value = !globalShortcutEnabled.value;
  const enabled = await saveGlobalShortcutEnabled();
  if (enabled !== null) toast(enabled ? "已启用快捷键 Alt + D" : "已关闭快捷键 Alt + D");
}

async function updateTopTriggerSettings() {
  try {
    await saveTopTriggerSettings();
  } catch (e) {
    toast(`顶部识别设置失败：${e}`);
  }
}

async function resetTopTriggerSettings() {
  topTriggerWidth.value = DEFAULT_TOP_TRIGGER_WIDTH;
  topTriggerDwellMs.value = DEFAULT_TOP_TRIGGER_DWELL_MS;
  await updateTopTriggerSettings();
  toast("已恢复顶部识别默认设置");
}

function startWindowDrag(event: MouseEvent) {
  if (event.button !== 0) return;
  menuOpen.value = false;
  invoke("start_window_drag_command").catch((e) => toast(`移动窗口失败：${e}`));
}
</script>

<template>
  <header class="p-head">
    <div class="brand">
      <span class="logo">✓</span>
      <div class="brand-text">
        <b>DBan</b>
        <span>{{ dateText }}</span>
      </div>
    </div>
    <div class="head-actions">
      <button
        class="icon-btn move-window-btn"
        data-tooltip="沿屏幕顶部移动"
        aria-label="沿屏幕顶部移动"
        @mousedown.prevent="startWindowDrag"
      >
        <svg class="icon" viewBox="0 0 24 24"><path d="M12 3v18M3 12h18M8 7l4-4 4 4M8 17l4 4 4-4M7 8l-4 4 4 4M17 8l4 4-4 4" /></svg>
      </button>
      <button class="icon-btn" :class="{ active: pinned }" data-tooltip="窗口置顶 开/关" aria-label="窗口置顶 开/关" @click="togglePin">
        <svg class="icon" viewBox="0 0 24 24"><path d="M9 3h6v5l2 3v1H7v-1l2-3V3z" /><path d="M12 12v6" /></svg>
      </button>
      <button class="icon-btn" data-tooltip="收起为胶囊条" aria-label="收起为胶囊条" @click="setMode('capsule')">
        <svg class="icon" viewBox="0 0 24 24"><path d="M6 12h12" /></svg>
      </button>
      <button class="icon-btn" data-tooltip="隐藏面板（Alt + D）" aria-label="隐藏面板" @click="setMode('hidden')">
        <svg class="icon" viewBox="0 0 24 24"><path d="M6 15l6-6 6 6" /></svg>
      </button>
      <button class="icon-btn" data-tooltip="设置" aria-label="设置" @click="menuOpen = !menuOpen">
        <svg class="icon" viewBox="0 0 24 24" fill="currentColor" stroke="none">
          <circle cx="5" cy="12" r="1.5" /><circle cx="12" cy="12" r="1.5" /><circle cx="19" cy="12" r="1.5" />
        </svg>
      </button>

      <div v-if="menuOpen" class="menu settings-menu" @mouseleave="menuOpen = false">
        <button @click="toggleTheme">
          <span class="check">{{ theme === "dark" ? "●" : "○" }}</span>
          {{ theme === "dark" ? "切换到亮白" : "切换到暗黑" }}
        </button>
        <button @click="toggleAutostart">
          <span class="check">{{ autostart ? "✓" : "" }}</span>
          开机自启
        </button>
        <div class="menu-section-title">快捷键</div>
        <button @click="toggleGlobalShortcut">
          <span class="check">{{ globalShortcutEnabled ? "✓" : "" }}</span>
          <span class="menu-label">呼出 / 隐藏</span>
          <kbd>Alt + D</kbd>
        </button>
        <div class="menu-section-title trigger-title">
          <span>顶部识别</span>
          <button class="settings-reset" data-tooltip="恢复为 360px / 250ms" @click.stop="resetTopTriggerSettings">
            重置默认
          </button>
        </div>
        <div class="range-setting">
          <div class="range-label">
            <span>区域宽度 <small>建议 320–480px</small></span>
            <output>{{ topTriggerWidth }}px</output>
          </div>
          <input
            v-model.number="topTriggerWidth"
            type="range"
            min="160"
            max="800"
            step="20"
            aria-label="顶部识别区域宽度"
            @change="updateTopTriggerSettings"
          />
        </div>
        <div class="range-setting">
          <div class="range-label">
            <span>停留时间 <small>建议 200–350ms</small></span>
            <output>{{ topTriggerDwellMs }}ms</output>
          </div>
          <input
            v-model.number="topTriggerDwellMs"
            type="range"
            min="100"
            max="1000"
            step="50"
            aria-label="顶部识别停留时间"
            @change="updateTopTriggerSettings"
          />
        </div>
        <button @click="invoke('set_mode_command', { mode: 'hidden' }); menuOpen = false">
          <span class="check"></span>
          隐藏面板
        </button>
      </div>
    </div>
  </header>
</template>
