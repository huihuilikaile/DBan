<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { todos, track, setMode } from "../store";

const pending = computed(() => todos.value.filter((t) => !t.done));
const expanded = ref(false);
let nativeExpanded = false;
let desiredExpanded = false;
let resizeTask: Promise<void> | undefined;

// 展开时的窗口总高：胶囊 48 + 弹窗（标题 + 每条 30 + 内边距），封顶 560
const winHeight = computed(() => Math.min(86 + pending.value.length * 30, 560));

async function applySize(nextExpanded: boolean) {
  try {
    await invoke("set_capsule_expanded_command", { expanded: nextExpanded, height: winHeight.value });
  } catch {
    /* 尺寸调整失败不影响显示 */
  }
}

async function reconcileSize() {
  while (nativeExpanded !== desiredExpanded) {
    const target = desiredExpanded;

    if (!target) {
      expanded.value = false;
      await nextTick();
      if (desiredExpanded !== target) continue;
    }

    await applySize(target);
    nativeExpanded = target;
  }
}

function requestSize(nextExpanded: boolean) {
  desiredExpanded = nextExpanded;
  if (!resizeTask) {
    resizeTask = reconcileSize().finally(() => {
      resizeTask = undefined;
      if (nativeExpanded !== desiredExpanded) requestSize(desiredExpanded);
    });
  }
  return resizeTask;
}

async function toggleTodos() {
  const target = !desiredExpanded;
  if (!target) {
    expanded.value = false;
  }
  await requestSize(target);
  if (target && desiredExpanded && nativeExpanded) expanded.value = true;
}

const text = computed(() =>
  track.value ? `♪ ${track.value.title || "未知曲目"}` : "♪ 未在播放"
);
</script>

<template>
  <div class="capsule-root">
    <div class="capsule" @click="setMode('panel')">
      <span class="c-logo">✓</span>
      <span class="c-eq" :class="{ paused: !track?.playing }"><i /><i /><i /></span>
      <span class="c-txt"><b>{{ pending.length }} 待办</b> · {{ text }}</span>
      <button
        class="c-toggle"
        :class="{ expanded }"
        :aria-label="expanded ? '隐藏待办' : '显示待办'"
        :aria-expanded="expanded"
        @click.stop="toggleTodos"
      >
        <svg class="icon" viewBox="0 0 24 24"><path d="m7 10 5 5 5-5" /></svg>
      </button>
    </div>

    <div v-if="expanded" class="capsule-pop">
      <div class="cp-title">待办 · {{ pending.length }} 项未完成</div>
      <div v-if="!pending.length" class="cp-empty">暂无待办，干得漂亮 ✓</div>
      <div v-for="t in pending.slice(0, 12)" :key="t.id" class="cp-item">
        <span class="dot" />
        <span class="cp-txt">{{ t.text }}</span>
      </div>
    </div>
  </div>
</template>
