<script setup lang="ts">
import { useWindowControls } from "../composables/useWindowControls";

const { isMaximized, minimize, toggleMaximize, close, startDrag } = useWindowControls();

function onPointerDown() {
  // 只在主拖拽区触发，按钮区域不拦截
  void startDrag();
}

function onButtonDown(event: MouseEvent) {
  // 阻止冒泡，避免点按钮时顺带触发拖拽
  event.stopPropagation();
}
</script>

<template>
  <!--
    品牌 + 窗口控制都在这条栏里：左侧 logo 与名称，右侧三个按钮。
    侧边栏从这条栏下沿直接开始放导航，不再有自己的头部。
  -->
  <div class="titlebar" @pointerdown="onPointerDown">
    <div class="titlebar__brand">
      <img class="titlebar__logo" src="../assets/logo.png" alt="" width="20" height="20" />
      <span class="titlebar__name">MC Speed</span>
      <span class="titlebar__tag">本地加速代理</span>
    </div>

    <!-- 品牌与按钮之间的空白也是拖拽区 -->
    <div class="titlebar__drag" aria-hidden="true" />

    <div class="titlebar__right" @pointerdown="onButtonDown">
      <button
        class="titlebar__btn"
        type="button"
        title="最小化"
        aria-label="最小化"
        @click="minimize"
      >
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
          <rect x="1.5" y="5.5" width="9" height="1" fill="currentColor" />
        </svg>
      </button>

      <button
        class="titlebar__btn"
        type="button"
        :title="isMaximized ? '还原' : '最大化'"
        :aria-label="isMaximized ? '还原' : '最大化'"
        @click="toggleMaximize"
      >
        <svg v-if="!isMaximized" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
          <rect x="1.5" y="1.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
        <svg v-else viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
          <rect x="1.5" y="3.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1" />
          <rect x="3.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>

      <button
        class="titlebar__btn titlebar__btn--danger"
        type="button"
        title="关闭"
        aria-label="关闭"
        @click="close"
      >
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
          <path
            d="M2 2 L10 10 M10 2 L2 10"
            fill="none"
            stroke="currentColor"
            stroke-width="1.2"
          />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
/*
 * 顶栏同时承担品牌区和窗口控制条。高度与侧边栏导航行的视觉节奏对齐，
 * 背景取半透明黑，和下方侧边栏的底色一致，接缝看不出来。
 */
.titlebar {
  display: flex;
  align-items: stretch;
  height: var(--titlebar-height, 40px);
  background: rgba(0, 0, 0, 0.28);
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  user-select: none;
  flex: 0 0 auto;
  width: 100%;
}

.titlebar__brand {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 14px;
  flex: 0 0 auto;
  min-width: 0;
}

.titlebar__logo {
  width: 20px;
  height: 20px;
  object-fit: contain;
  flex: 0 0 auto;
}

.titlebar__name {
  font-family: "Minecraft Ten", system-ui, sans-serif;
  font-size: 13px;
  letter-spacing: 0.02em;
  white-space: nowrap;
}

.titlebar__tag {
  font-size: 11px;
  opacity: 0.45;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 品牌与按钮之间的空白也是拖拽区，不可见也不放内容 */
.titlebar__drag {
  flex: 1 1 auto;
  min-width: 0;
  cursor: default;
}

.titlebar__right {
  display: flex;
  align-items: stretch;
  flex: 0 0 auto;
}

.titlebar__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 100%;
  border: 0;
  background: transparent;
  color: inherit;
  cursor: pointer;
  opacity: 0.75;
  transition: background-color 120ms ease, opacity 120ms ease;
}

.titlebar__btn:hover {
  background: rgba(255, 255, 255, 0.1);
  opacity: 1;
}

.titlebar__btn--danger:hover {
  background: #c42b1c;
  opacity: 1;
  color: #fff;
}

.titlebar__btn:active {
  transform: translateY(1px);
}
</style>
