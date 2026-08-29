<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { vaults, saveVaults, toast } from "../store";
import type { VaultItem } from "../store";

const showForm = ref(false);
const site = ref("");
const account = ref("");
const password = ref("");

// 明文短暂显示（5 秒）
const revealedId = ref("");
const revealedPw = ref("");
const revealLoadingId = ref("");
const pendingDelete = ref<VaultItem | null>(null);
const deleting = ref(false);
let revealTimer: ReturnType<typeof setTimeout> | undefined;

async function add() {
  const s = site.value.trim();
  const a = account.value.trim();
  const p = password.value;
  if (!s || !p) {
    toast("站点和密码不能为空");
    return;
  }
  const id = crypto.randomUUID();
  try {
    await invoke("save_secret", { id, secret: p });
  } catch (e) {
    toast(`保存失败：${e}`);
    return;
  }
  vaults.value.push({ id, site: s, account: a });
  await saveVaults();
  site.value = account.value = password.value = "";
  showForm.value = false;
  toast("已保存：密码写入 Windows 凭据管理器");
}

async function copyItem(id: string, siteName: string) {
  try {
    await invoke("copy_secret", { id });
    toast(`已复制「${siteName}」密码，30 秒后自动清空剪贴板`);
  } catch (e) {
    toast(String(e));
  }
}

function hideReveal() {
  clearTimeout(revealTimer);
  revealTimer = undefined;
  revealedId.value = "";
  revealedPw.value = "";
}

async function toggleReveal(item: { id: string }) {
  // 眼睛 = 切换显示/隐藏；显示后 5 秒也会自动隐藏
  if (revealedId.value === item.id) {
    hideReveal();
    return;
  }
  if (revealLoadingId.value) return;
  revealLoadingId.value = item.id;
  try {
    const pw = await invoke<string | null>("get_secret", { id: item.id });
    if (!pw) {
      toast("未找到该密码，请删除条目后重新保存");
      return;
    }
    revealedId.value = item.id;
    revealedPw.value = pw;
    clearTimeout(revealTimer);
    revealTimer = setTimeout(hideReveal, 5000);
  } catch (e) {
    toast(`读取密码失败：${e}`);
  } finally {
    revealLoadingId.value = "";
  }
}

function requestRemove(item: VaultItem) {
  hideReveal();
  pendingDelete.value = item;
}

function cancelRemove() {
  if (!deleting.value) pendingDelete.value = null;
}

async function confirmRemove() {
  const item = pendingDelete.value;
  if (!item || deleting.value) return;
  deleting.value = true;
  try {
    await invoke("delete_secret", { id: item.id });
    vaults.value = vaults.value.filter((v) => v.id !== item.id);
    await saveVaults();
    pendingDelete.value = null;
    toast(`已删除「${item.site}」`);
  } catch (e) {
    toast(`删除失败：${e}`);
  } finally {
    deleting.value = false;
  }
}

onBeforeUnmount(hideReveal);
</script>

<template>
  <section class="pane">
    <div class="vault-tip">
      密码加密存放于 Windows 凭据管理器（DPAPI），本列表仅保存条目信息；复制 30 秒后自动清空剪贴板。
    </div>

    <div v-if="showForm" class="vform">
      <input v-model="site" placeholder="站点名称，如 知乎" maxlength="24" />
      <input v-model="account" placeholder="账号 / 邮箱（可选）" maxlength="60" />
      <input v-model="password" type="password" placeholder="密码" maxlength="120" />
      <div class="row">
        <button @click="showForm = false">取消</button>
        <button class="primary" @click="add">保存</button>
      </div>
    </div>

    <ul class="vault-list">
      <li v-for="v in vaults" :key="v.id" class="vault">
        <div class="vmeta">
          <b>{{ v.site }}</b>
          <span class="acc">{{ v.account || "—" }}</span>
          <span v-if="revealedId === v.id" class="vpw">{{ revealedPw }}</span>
        </div>
        <div class="vact">
          <button data-tooltip="复制密码" aria-label="复制密码" @click="copyItem(v.id, v.site)">
            <svg class="icon" viewBox="0 0 24 24"><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M5 15V6a2 2 0 0 1 2-2h9" /></svg>
          </button>
          <button
            :data-tooltip="revealedId === v.id ? '隐藏密码' : '显示密码（5 秒后自动隐藏）'"
            :aria-label="revealedId === v.id ? '隐藏密码' : '显示密码'"
            :disabled="Boolean(revealLoadingId)"
            @click="toggleReveal(v)"
          >
            <svg v-if="revealedId !== v.id" class="icon" viewBox="0 0 24 24"><path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6S2 12 2 12z" /><circle cx="12" cy="12" r="2.6" /></svg>
            <svg v-else class="icon" viewBox="0 0 24 24"><path d="M3 3l18 18M10.6 6.2A10.6 10.6 0 0 1 12 6c6.5 0 10 6 10 6a17 17 0 0 1-2.2 2.9M6.6 6.6C3.6 8.3 2 12 2 12s3.5 6 10 6c1 0 2-.2 2.8-.4M9.9 9.9a3 3 0 0 0 4.2 4.2" /></svg>
          </button>
          <button data-tooltip="删除" aria-label="删除" @click="requestRemove(v)">
            <svg class="icon" viewBox="0 0 24 24"><path d="M4 7h16M9 7V4h6v3M6.5 7l1 13h9l1-13" /></svg>
          </button>
        </div>
      </li>
    </ul>

    <button v-if="!showForm" class="addtile" @click="showForm = true">＋ 添加账号密码</button>

    <div v-if="pendingDelete" class="modal-backdrop" role="presentation" @click.self="cancelRemove">
      <section class="confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-title">
        <div class="confirm-icon" aria-hidden="true">
          <svg class="icon" viewBox="0 0 24 24"><path d="M12 9v4M12 17h.01M10.3 3.7 2.6 17a2 2 0 0 0 1.7 3h15.4a2 2 0 0 0 1.7-3L13.7 3.7a2 2 0 0 0-3.4 0z" /></svg>
        </div>
        <h2 id="delete-title">删除密码记录？</h2>
        <p>将同时删除「{{ pendingDelete.site }}」在 Windows 凭据管理器中的密码，此操作无法撤销。</p>
        <div class="confirm-actions">
          <button :disabled="deleting" @click="cancelRemove">取消</button>
          <button class="danger" :disabled="deleting" @click="confirmRemove">
            {{ deleting ? "删除中..." : "确认删除" }}
          </button>
        </div>
      </section>
    </div>
  </section>
</template>
