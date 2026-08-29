<script setup lang="ts">
import { computed } from "vue";
import { todoHistory } from "../store";

const history = computed(() => [...todoHistory.value].sort((a, b) => b.clearedAt - a.clearedAt));

function formatTime(timestamp: number) {
  const date = new Date(timestamp);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}.${pad(date.getMonth() + 1)}.${pad(date.getDate())}/${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
</script>

<template>
  <section class="pane history-pane">
    <div v-if="!history.length" class="history-empty">
      暂无已清除的完成待办
    </div>
    <ol v-else class="history-list">
      <li v-for="item in history" :key="item.id" class="history-item">
        <span class="history-check" aria-hidden="true">
          <svg class="icon" viewBox="0 0 24 24"><path d="M20 6L9 17l-5-5" /></svg>
        </span>
        <div class="history-content">
          <span class="history-text">{{ item.text }}</span>
          <time>{{ formatTime(item.createdAt) }} -&gt; {{ formatTime(item.clearedAt) }}</time>
        </div>
      </li>
    </ol>
  </section>
</template>
