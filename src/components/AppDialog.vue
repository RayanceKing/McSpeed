<script setup lang="ts">
import { dialogRef, closeDialog } from "../composables/useDialog";

const state = dialogRef();
</script>

<template>
  <mc-dialog v-model="state.open" :title="state.title" :width="460">
    <p class="dialog-message">{{ state.message }}</p>

    <template #actions>
      <mc-button variant="primary" @click="closeDialog">
        {{ state.actionText }}
      </mc-button>
    </template>
  </mc-dialog>
</template>

<style scoped>
.dialog-message {
  font-size: 13px;
  line-height: 1.7;
  opacity: 0.9;
  margin: 0;
  white-space: pre-wrap;
  /* 长路径和错误信息需要能换行 */
  overflow-wrap: anywhere;
}
</style>
