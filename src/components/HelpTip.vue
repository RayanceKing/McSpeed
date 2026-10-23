<script setup lang="ts">
/**
 * 帮助提示：一个问号图标，悬停或聚焦时弹出说明文本。
 *
 * 用于替代小标题旁边那一长串描述文字——既占地方又没人看，
 * 收进 tooltip 后标题能保持一行，需要时鼠标放上去就有。
 */
defineProps<{
  /** 帮助文本 */
  text: string;
  /** 图标尺寸（px） */
  size?: number;
}>();
</script>

<template>
  <mc-tooltip :content="text" placement="top" class="help-tip">
    <mc-icon
      name="mc-question-mark"
      :size="size ?? 16"
      class="help-tip__icon"
      role="img"
      :aria-label="`帮助：${text}`"
    />
  </mc-tooltip>
</template>

<style scoped>
.help-tip {
  display: inline-flex;
  align-items: center;
  vertical-align: middle;
  cursor: help;
  /* 图标是装饰性的，别抢走旁边文字的可点击区域 */
  flex: 0 0 auto;
}

.help-tip__icon {
  opacity: 0.45;
  transition: opacity 120ms ease;
}

.help-tip:hover .help-tip__icon,
.help-tip:focus-within .help-tip__icon {
  opacity: 0.9;
}
</style>
