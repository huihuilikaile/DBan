<script setup lang="ts">
import { mediaCycleMode, mediaNext, mediaPrev, mediaToggle } from "../store";
import type { Track } from "../store";

defineProps<{ track: Track | null }>();
</script>

<template>
  <div class="player">
    <span class="p-eq" :class="{ playing: track?.playing }" aria-hidden="true">
      <i /><i /><i /><i />
    </span>
    <span class="p-mini">
      <template v-if="track">
        <b>{{ track.title || "未知曲目" }}</b> · {{ track.artist || "未知歌手" }}
      </template>
      <template v-else>未捕获到正在播放的音乐</template>
    </span>
    <div class="ctrl">
      <button
        class="play-mode"
        :class="{ active: track?.playMode && track.playMode !== 'sequence' }"
        :data-tooltip="track?.playMode === 'single' ? '单曲循环' : track?.playMode === 'shuffle' ? '随机播放' : '顺序播放'"
        aria-label="切换播放模式"
        @click="mediaCycleMode"
      >
        <svg v-if="track?.playMode === 'shuffle'" class="icon" viewBox="0 0 24 24"><path d="M3 7h3c4 0 5 10 9 10h5M17 14l3 3-3 3M3 17h3c1.8 0 3-2 4-4M14 7h6M17 4l3 3-3 3" /></svg>
        <svg v-else class="icon" viewBox="0 0 24 24"><path d="M17 2l4 4-4 4M3 11V9a3 3 0 0 1 3-3h15M7 22l-4-4 4-4M21 13v2a3 3 0 0 1-3 3H3" /><text v-if="track?.playMode === 'single'" x="12" y="14.5">1</text></svg>
      </button>
      <button data-tooltip="上一首" aria-label="上一首" @click="mediaPrev">
        <svg class="icon" viewBox="0 0 24 24" fill="currentColor" stroke="none">
          <path d="M7 5h2v14H7z" /><path d="M19 5l-9 7 9 7z" />
        </svg>
      </button>
      <button class="play" :data-tooltip="track?.playing ? '暂停' : '播放'" :aria-label="track?.playing ? '暂停' : '播放'" @click="mediaToggle">
        <svg v-if="track?.playing" class="icon" viewBox="0 0 24 24" fill="currentColor" stroke="none">
          <path d="M7 5h3.4v14H7zM13.6 5H17v14h-3.6z" />
        </svg>
        <svg v-else class="icon" viewBox="0 0 24 24" fill="currentColor" stroke="none">
          <path d="M8 5l11 7-11 7z" />
        </svg>
      </button>
      <button data-tooltip="下一首" aria-label="下一首" @click="mediaNext">
        <svg class="icon" viewBox="0 0 24 24" fill="currentColor" stroke="none">
          <path d="M15 5h2v14h-2z" /><path d="M5 5l9 7-9 7z" />
        </svg>
      </button>
    </div>
  </div>
</template>
