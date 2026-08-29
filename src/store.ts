import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Store } from "@tauri-apps/plugin-store";

export interface Todo { id: string; text: string; done: boolean; createdAt: number }
export interface TodoHistoryItem { id: string; text: string; createdAt: number; clearedAt: number }
export interface VaultItem { id: string; site: string; account: string }
export interface AppItem { id: string; name: string; path: string; icon: string; categoryId?: string }
export interface AppCategory { id: string; name: string }
export type Mode = "panel" | "capsule" | "hidden";
export type PlayMode = "sequence" | "single" | "shuffle";
export interface Track { title: string; artist: string; playing: boolean; playMode: PlayMode }
export type Theme = "dark" | "light";

export const todos = ref<Todo[]>([]);
export const todoHistory = ref<TodoHistoryItem[]>([]);
export const vaults = ref<VaultItem[]>([]);
export const apps = ref<AppItem[]>([]);
export const appCategories = ref<AppCategory[]>([]);
export const activeAppCategoryId = ref("all");
export const theme = ref<Theme>("dark");
export const pinned = ref(true);
export const autostart = ref(false);
export const globalShortcutEnabled = ref(true);
export const topTriggerWidth = ref(360);
export const topTriggerDwellMs = ref(250);
export const mode = ref<Mode>("hidden");
export const track = ref<Track | null>(null);

export const DEFAULT_TOP_TRIGGER_WIDTH = 360;
export const DEFAULT_TOP_TRIGGER_DWELL_MS = 250;

/* ---------- toast ---------- */
export const toastMsg = ref("");
let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(msg: string) {
  toastMsg.value = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toastMsg.value = ""), 2600);
}

/* ---------- 持久化 ---------- */
let store: Promise<Store> | null = null;
function getStore() {
  store ??= Store.load("dban.json");
  return store;
}

const SEED_TODOS: Todo[] = [
  { id: "seed-1", text: "把面板停靠到屏幕右上角", done: true, createdAt: Date.now() },
  { id: "seed-2", text: "鼠标移到屏幕顶部试试呼出 / Alt+D", done: false, createdAt: Date.now() },
  { id: "seed-3", text: "添加一个待办事项", done: false, createdAt: Date.now() },
];

export async function initStore() {
  const s = await getStore();
  const storedTodos = (await s.get<Array<Partial<Todo> & Pick<Todo, "id" | "text" | "done">>>("todos")) ?? SEED_TODOS;
  const migrationTime = Date.now();
  todos.value = storedTodos.map((todo) => ({
    ...todo,
    createdAt: typeof todo.createdAt === "number" ? todo.createdAt : migrationTime,
  }));
  todoHistory.value = (await s.get<TodoHistoryItem[]>("todoHistory")) ?? [];
  vaults.value = (await s.get<VaultItem[]>("vaults")) ?? [];
  apps.value = (await s.get<AppItem[]>("apps")) ?? [];
  appCategories.value = (await s.get<AppCategory[]>("appCategories")) ?? [];
  const savedAppCategoryId = (await s.get<string>("activeAppCategoryId")) ?? "all";
  activeAppCategoryId.value = savedAppCategoryId === "all" || appCategories.value.some((c) => c.id === savedAppCategoryId)
    ? savedAppCategoryId
    : "all";
  theme.value = (await s.get<Theme>("theme")) ?? "dark";
  pinned.value = (await s.get<boolean>("pinned")) ?? true;
  globalShortcutEnabled.value = (await s.get<boolean>("globalShortcutEnabled")) ?? true;
  topTriggerWidth.value = Math.min(800, Math.max(160,
    (await s.get<number>("topTriggerWidth")) ?? DEFAULT_TOP_TRIGGER_WIDTH,
  ));
  topTriggerDwellMs.value = Math.min(1000, Math.max(100,
    (await s.get<number>("topTriggerDwellMs")) ?? DEFAULT_TOP_TRIGGER_DWELL_MS,
  ));
  await Promise.all([
    saveTodos(), saveTodoHistory(), saveVaults(), saveApps(), saveAppCategories(), saveActiveAppCategory(), saveTheme(), savePinned(),
    saveGlobalShortcutEnabled(), saveTopTriggerSettings(),
  ]);
}

export async function saveTodos() {
  (await getStore()).set("todos", todos.value);
}
export async function saveTodoHistory() {
  (await getStore()).set("todoHistory", todoHistory.value);
}
export async function saveVaults() {
  (await getStore()).set("vaults", vaults.value);
}
export async function saveApps() {
  (await getStore()).set("apps", apps.value);
}
export async function saveAppCategories() {
  (await getStore()).set("appCategories", appCategories.value);
}
export async function saveActiveAppCategory() {
  (await getStore()).set("activeAppCategoryId", activeAppCategoryId.value);
}
export async function saveTheme() {
  (await getStore()).set("theme", theme.value);
  document.body.classList.toggle("light", theme.value === "light");
}
export async function savePinned() {
  (await getStore()).set("pinned", pinned.value);
  await invoke("set_pinned_command", { pinned: pinned.value });
}
export async function saveGlobalShortcutEnabled() {
  const s = await getStore();
  try {
    globalShortcutEnabled.value = await invoke<boolean>("set_global_shortcut_enabled_command", {
      enabled: globalShortcutEnabled.value,
    });
    await s.set("globalShortcutEnabled", globalShortcutEnabled.value);
    return globalShortcutEnabled.value;
  } catch (e) {
    globalShortcutEnabled.value = false;
    await s.set("globalShortcutEnabled", false);
    toast(`快捷键设置失败：${e}`);
    return null;
  }
}
export async function saveTopTriggerSettings() {
  topTriggerWidth.value = Math.min(800, Math.max(160, Math.round(topTriggerWidth.value / 20) * 20));
  topTriggerDwellMs.value = Math.min(1000, Math.max(100, Math.round(topTriggerDwellMs.value / 50) * 50));
  const s = await getStore();
  await Promise.all([
    s.set("topTriggerWidth", topTriggerWidth.value),
    s.set("topTriggerDwellMs", topTriggerDwellMs.value),
    invoke("set_top_trigger_settings_command", {
      width: topTriggerWidth.value,
      dwellMs: topTriggerDwellMs.value,
    }),
  ]);
}

/* ---------- 模式 ---------- */
export async function setMode(m: Mode) {
  mode.value = m;
  try {
    await invoke("set_mode_command", { mode: m });
  } catch (e) {
    toast(String(e));
  }
}

let eventUnlisteners: UnlistenFn[] = [];

export async function initEvents() {
  disposeEvents();
  const unlistenMode = await listen<Mode>("dban://mode", (e) => {
    mode.value = e.payload;
  });
  const unlistenAutostart = await listen<boolean>("dban://autostart", (e) => {
    autostart.value = e.payload;
  });
  const unlistenMedia = await listen<Track | null>("media://update", (e) => {
    track.value = e.payload;
  });
  eventUnlisteners = [unlistenMode, unlistenAutostart, unlistenMedia];
  autostart.value = await invoke<boolean>("get_autostart_command");
}

export function disposeEvents() {
  for (const unlisten of eventUnlisteners) unlisten();
  eventUnlisteners = [];
}

export async function setAutostart(enabled: boolean) {
  try {
    autostart.value = await invoke<boolean>("set_autostart_command", { enabled });
    return autostart.value;
  } catch (e) {
    toast(String(e));
    return null;
  }
}

/* ---------- 媒体控制 ---------- */
export async function mediaToggle() {
  if (!track.value) return;
  track.value = { ...track.value, playing: !track.value.playing }; // 乐观更新，轮询事件会纠正
  try {
    await invoke("media_toggle");
  } catch (e) {
    toast(String(e));
  }
}
export async function mediaNext() {
  try { await invoke("media_next"); } catch (e) { toast(String(e)); }
}
export async function mediaPrev() {
  try { await invoke("media_prev"); } catch (e) { toast(String(e)); }
}
export async function mediaCycleMode() {
  if (!track.value) {
    toast("没有可控制的媒体会话");
    return;
  }
  const next: Record<PlayMode, PlayMode> = {
    sequence: "single",
    single: "shuffle",
    shuffle: "sequence",
  };
  const labels: Record<PlayMode, string> = {
    sequence: "顺序播放",
    single: "单曲循环",
    shuffle: "随机播放",
  };
  const requested = next[track.value.playMode ?? "sequence"];
  try {
    const actual = await invoke<PlayMode>("media_set_mode", { mode: requested });
    track.value = { ...track.value, playMode: actual };
    toast(`已切换为${labels[actual]}`);
  } catch (e) {
    toast(String(e));
  }
}
