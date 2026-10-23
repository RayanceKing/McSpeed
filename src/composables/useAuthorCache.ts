import { ref } from "vue";
import { getAuthorInfo } from "../api";
import type { AuthorInfo } from "../types";

/**
 * 作者信息的全局缓存。
 *
 * 放在模块级而不是组件里：页面切换会销毁重建 AboutPage，
 * 每次 onMounted 都拉一遍接口太浪费，也没必要。
 */
const cache = ref<AuthorInfo | null>(null);

/** 正在拉取的 Promise，防止并发重复请求。 */
let pending: Promise<void> | null = null;

/** 读取缓存；没有就发起一次拉取。 */
export function ensureAuthor(): Promise<void> {
  if (cache.value) return Promise.resolve();
  if (pending) return pending;

  pending = getAuthorInfo()
    .then((info) => {
      cache.value = info;
    })
    .catch(() => {
      // 拉取失败保持 null，下次进页面重试；不缓存错误
    })
    .finally(() => {
      pending = null;
    });
  return pending;
}

export function useAuthorCache() {
  return { author: cache };
}
