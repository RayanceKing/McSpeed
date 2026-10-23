<script setup lang="ts">
/**
 * 侧边导航栏。
 *
 * 不用 McDrawer：它是 position:fixed，在纵向的 mc-layout 里不会占位，
 * 会浮在内容上面。这里自己实现一个 flex row 里的固定宽度列，
 * 用 McList 的选中态表达当前页面。
 */
defineProps<{
  modelValue: string;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: string): void;
}>();

const items = [
  { value: "overview", label: "总览", icon: "mc-home" },
  { value: "domains", label: "加速规则", icon: "mc-blocks" },
  { value: "hosts", label: "hosts", icon: "mc-clipboard" },
  { value: "diagnostics", label: "诊断", icon: "mc-debug" },
  { value: "about", label: "关于", icon: "mc-information" },
];

/**
 * McList 的 modelValue 类型是 McListValue | McListValue[]，
 * 单选模式下实际只会传单个值，这里收窄成 string。
 */
function onSelect(value: unknown) {
  if (typeof value === "string") {
    emit("update:modelValue", value);
  }
}
</script>

<template>
  <!--
    品牌区在顶栏，这里只放导航列表。
    侧边栏从顶栏下沿直接开始，不再有自己的头部色块。
  -->
  <nav class="sidenav" aria-label="主导航">
    <!--
      mode 必须显式给值：库的选中态（mc-list__item--active）只在 mode 非空时
      生效，否则点了也不会有任何视觉变化。single 模式自带单选框，
      导航不需要，用 :show-radio="false" 关掉，选中效果改由下面的样式承担。
    -->
    <mc-list
      class="sidenav__list"
      :model-value="modelValue"
      mode="single"
      :show-radio="false"
      @update:model-value="onSelect"
    >
      <mc-list-item
        v-for="item in items"
        :key="item.value"
        :label="item.label"
        :value="item.value"
        :icon="item.icon"
      />
    </mc-list>
  </nav>
</template>

<style scoped>
.sidenav {
  display: flex;
  flex-direction: column;
  /* 固定宽度，不随内容伸缩 */
  flex: 0 0 208px;
  width: 208px;
  background: rgba(0, 0, 0, 0.28);
  border-inline-end: 1px solid rgba(255, 255, 255, 0.08);
  min-height: 0;
  overflow-y: auto;
}

/* 导航列表贴顶开始，品牌区已在顶栏里 */
.sidenav__list {
  flex: 1 1 auto;
  min-height: 0;
  padding: 8px 8px;
}

/* 列表项之间留点呼吸空间 */
:deep(.mc-list__item) {
  margin-bottom: 2px;
  transition:
    background-color 0.12s ease,
    border-color 0.12s ease,
    box-shadow 0.12s ease;
}

/*
 * 选中态：绿底 + 绿框 + 左侧竖条，比库默认的按压灰块醒目。
 * 选择器带齐三个类压过库的 :hover 规则，鼠标停在选中项上不会变回白框。
 */
:deep(.mc-list__item.mc-list__item--interactive.mc-list__item--active) {
  background: rgba(60, 133, 39, 0.22);
  border-color: #3c8527;
  box-shadow:
    inset 2px 2px rgba(255, 255, 255, 0.12),
    inset -2px -2px rgba(0, 0, 0, 0.3);
}

:deep(.mc-list__item.mc-list__item--interactive.mc-list__item--active)::before {
  content: "";
  position: absolute;
  left: 0;
  top: 6px;
  bottom: 6px;
  width: 3px;
  background: #3c8527;
}
</style>
