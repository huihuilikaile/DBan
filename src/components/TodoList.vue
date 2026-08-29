<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { todoHistory, todos, saveTodoHistory, saveTodos, toast } from "../store";
import type { Todo } from "../store";

const input = ref("");
const editingId = ref("");
const editText = ref("");
const now = ref(Date.now());
let ageTimer: ReturnType<typeof setInterval> | undefined;

const doneCount = computed(() => todos.value.filter((t) => t.done).length);

function formatAge(createdAt: number) {
  const totalMinutes = Math.max(0, Math.floor((now.value - createdAt) / 60_000));
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  if (days > 0) return `${days}天${hours}小时${minutes}分钟`;
  if (hours > 0) return `${hours}小时${minutes}分钟`;
  return `${minutes}分钟`;
}

onMounted(() => {
  ageTimer = setInterval(() => {
    now.value = Date.now();
  }, 60_000);
});

onBeforeUnmount(() => clearInterval(ageTimer));

function add() {
  const v = input.value.trim();
  if (!v) return;
  todos.value.push({ id: crypto.randomUUID(), text: v, done: false, createdAt: Date.now() });
  input.value = "";
  saveTodos();
}

function toggle(id: string) {
  const t = todos.value.find((t) => t.id === id);
  if (!t) return;
  t.done = !t.done;
  if (t.done) toast("已完成 ✓");
  saveTodos();
}

function remove(id: string) {
  if (editingId.value === id) cancelEdit();
  todos.value = todos.value.filter((t) => t.id !== id);
  saveTodos();
}

function startEdit(todo: Todo) {
  if (editingId.value === todo.id) return;
  if (editingId.value) commitEdit();
  editingId.value = todo.id;
  editText.value = todo.text;
  nextTick(() => {
    const element = document.querySelector<HTMLInputElement>(".todo-edit");
    element?.focus();
    element?.select();
  });
}

function commitEdit() {
  const id = editingId.value;
  if (!id) return;
  const todo = todos.value.find((t) => t.id === id);
  const text = editText.value.trim();
  if (todo && text) {
    todo.text = text;
    saveTodos();
  } else if (!text) {
    toast("待办内容不能为空");
  }
  editingId.value = "";
  editText.value = "";
}

function cancelEdit() {
  editingId.value = "";
  editText.value = "";
}

function clearDone() {
  const clearedAt = Date.now();
  const completed = todos.value.filter((t) => t.done);
  if (!completed.length) return;
  todoHistory.value.unshift(...completed.map((todo) => ({
    id: crypto.randomUUID(),
    text: todo.text,
    createdAt: todo.createdAt,
    clearedAt,
  })));
  todos.value = todos.value.filter((t) => !t.done);
  Promise.all([saveTodos(), saveTodoHistory()]);
  toast(`已将 ${completed.length} 项移入历史`);
}
</script>

<template>
  <section class="pane">
    <div class="addrow">
      <input
        v-model="input"
        placeholder="添加待办，回车确认…"
        maxlength="60"
        @keydown.enter.prevent="add"
      />
      <button class="addbtn" data-tooltip="添加" aria-label="添加" @click="add">
        <svg class="icon" viewBox="0 0 24 24"><path d="M12 5v14M5 12h14" /></svg>
      </button>
    </div>

    <ul class="todo-list">
      <li v-for="(t, index) in todos" :key="t.id" class="todo" :class="{ done: t.done, editing: editingId === t.id }" @click="startEdit(t)">
        <button class="chk" data-tooltip="完成 / 撤销" aria-label="完成或撤销" @click.stop="toggle(t.id)">
          <svg class="icon" viewBox="0 0 24 24"><path d="M20 6L9 17l-5-5" /></svg>
        </button>
        <input
          v-if="editingId === t.id"
          v-model="editText"
          class="todo-edit"
          maxlength="60"
          aria-label="编辑待办"
          @click.stop
          @blur="commitEdit"
          @keydown.enter.prevent="commitEdit"
          @keydown.escape.prevent="cancelEdit"
        />
        <span v-else class="txt">{{ t.text }}</span>
        <time class="todo-age" :datetime="new Date(t.createdAt).toISOString()">
          已创建 {{ formatAge(t.createdAt) }}
        </time>
        <button class="del" data-tooltip="删除" aria-label="删除" @click.stop="remove(t.id)">
          <svg class="icon" viewBox="0 0 24 24"><path d="M4 7h16M9 7V4h6v3M6.5 7l1 13h9l1-13" /></svg>
        </button>
        <span class="todo-index" :aria-label="`序号 ${index + 1}`">{{ index + 1 }}</span>
      </li>
    </ul>

    <div class="todo-foot">
      <span>{{ todos.length - doneCount }} 项待办 · {{ doneCount }} 已完成</span>
      <button :disabled="doneCount === 0" @click="clearDone">清除已完成</button>
    </div>
  </section>
</template>
