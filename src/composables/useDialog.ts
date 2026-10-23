import { ref } from "vue";

/**
 * 一个极简的全局信息框。
 *
 * 页面里要提示用户时，与其把消息堆在正文里，不如弹一个模态框——
 * 特别是失败原因这种需要用户明确看到并点击确认的信息。
 */
export interface DialogState {
  open: boolean;
  title: string;
  message: string;
  /** 按钮文字 */
  actionText: string;
}

const state = ref<DialogState>({
  open: false,
  title: "",
  message: "",
  actionText: "知道了",
});

/** 弹出信息框。可传 Error 或字符串。 */
export function showDialog(
  title: string,
  message: unknown,
  actionText = "知道了"
) {
  state.value = {
    open: true,
    title,
    message: toMessage(message),
    actionText,
  };
}

export function closeDialog() {
  state.value.open = false;
}

/** 返回 ref 本身，供模板直接解包使用。 */
export function dialogRef() {
  return state;
}

function toMessage(message: unknown): string {
  if (message instanceof Error) return message.message;
  if (typeof message === "string") return message;
  return String(message);
}
