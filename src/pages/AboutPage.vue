<script setup lang="ts">
import { onMounted } from "vue";
import HelpTip from "../components/HelpTip.vue";
import { openExternal } from "../composables/useOpenUrl";
import { ensureAuthor, useAuthorCache } from "../composables/useAuthorCache";

// 缓存在模块级：页面切换会销毁重建 AboutPage，重复拉接口没意义
const { author } = useAuthorCache();

onMounted(() => {
  void ensureAuthor();
});

/** 项目实际用到的框架与工具。 */
const links = [
  {
    label: "Tauri",
    url: "https://tauri.app",
    icon: "mc-blocks",
    hint: "桌面应用框架：Rust 后端 + 系统 WebView",
  },
  {
    label: "mcui-oreui",
    url: "https://github.com/ShenYuanOR/mcui-oreui",
    icon: "mc-palette",
    hint: "界面使用的 Minecraft Ore UI 风格 Vue 3 组件库",
  },
  {
    label: "Caddy",
    url: "https://caddyserver.com",
    icon: "mc-cloud-sync",
    hint: "本地反向代理，负责 TLS 与规则匹配",
  },
  {
    label: "mkcert",
    url: "https://github.com/FiloSottile/mkcert",
    icon: "mc-locked",
    hint: "给 Caddy 签发本地受信证书",
  },
  {
    label: "Cloudflare",
    url: "https://www.cloudflare.com/",
    icon: "mc-world",
    hint: "FabricProxy 部署的边缘平台",
  },
];

/**
 * 行点击回调。
 *
 * McList 是数据驱动的：McListItem 只是声明数据、自身渲染 null 且不发 click，
 * 行上的 @click 永远不会触发。点击统一从 McList 的 change 拿到行的 value（即 URL）。
 */
function onLinkChange(value: string | number | Array<string | number>) {
  const url = Array.isArray(value) ? value[0] : value;
  if (url != null) void openExternal(String(url));
}

</script>

<template>
  <div class="page">
    <!-- 基本信息 -->
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-information" :size="16" class="card-icon" />
          关于 MC Speed
          <HelpTip text="本地加速代理的管理工具。界面基于 Tauri + Vue 3，视觉使用 mcui-oreui 组件库。" />
        </span>
      </template>
      <template #text>
        <div class="brand">
          <img class="brand__logo" src="../assets/logo.png" alt="MC Speed" width="96" height="96" />
          <div class="brand__meta">
            <span class="brand__name">MC Speed</span>
            <span class="brand__tagline">本地加速代理管理工具</span>
          </div>
        </div>

        <div class="facts">
          <div class="fact">
            <span class="fact-key">版本</span>
            <span class="fact-val mono">2.0.0</span>
          </div>
        </div>
      </template>
    </mc-card>

    <!-- 作者 -->
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-friends" :size="16" class="card-icon" />
          作者
        </span>
      </template>
      <template #text>
        <!-- 加载中：骨架屏占位，布局与真实内容一一对应（头像 + 两行文字 + 两个链接行） -->
        <div v-if="!author" class="author author--skeleton" aria-hidden="true">
          <mc-skeleton width="72px" height="72px" class="author__avatar" />
          <div class="author__meta">
            <mc-skeleton width="140px" height="22px" />
            <mc-skeleton width="96px" height="14px" />
          </div>
        </div>

        <div v-else class="author">
          <img
            v-if="author.avatar"
            class="author__avatar"
            :src="author.avatar"
            alt="Microsoft / Minecraft 头像"
            width="72"
            height="72"
          />
          <div v-else class="author__avatar author__avatar--empty" aria-hidden="true">A</div>
          <div class="author__meta">
            <span class="brand__name">{{ author.name }}</span>
            <span class="author__ids mono">@{{ author.login }}</span>
          </div>
        </div>

        <!-- bio 永远存在（拉不到就回落快照），所以骨架也要留一行，避免加载完高度跳变 -->
        <div v-if="!author" class="author__lines" aria-hidden="true">
          <mc-skeleton width="100%" height="18px" />
        </div>
        <p v-else-if="author.bio" class="lead">{{ author.bio }}</p>

        <!-- 链接列表：骨架占两行，与「GitHub 主页 + 赞助支持」对齐 -->
        <mc-list v-if="!author" class="author__links" aria-hidden="true">
          <mc-list-item label=" " value="skeleton-1" :interactive="false">
            <template #right>
              <mc-skeleton width="16px" height="16px" />
            </template>
          </mc-list-item>
          <mc-list-item label=" " value="skeleton-2" :interactive="false">
            <template #right>
              <mc-skeleton width="16px" height="16px" />
            </template>
          </mc-list-item>
        </mc-list>

        <mc-list v-else class="author__links" @change="onLinkChange">
          <mc-list-item
            label="GitHub 主页"
            subtitle="源码与动态"
            :value="author.github_url"
            icon="mc-world"
            icon-right="mc-external-link"
          />
          <mc-list-item
            v-if="author.sponsor_url"
            label="赞助支持"
            subtitle="爱发电"
            :value="author.sponsor_url"
            icon="mc-heart-outline"
            icon-right="mc-external-link"
          />
        </mc-list>
      </template>
    </mc-card>

    <!-- 相关链接 -->
    <mc-card>
      <template #title>
        <span class="card-title">
          <mc-icon name="mc-external-link" :size="16" class="card-icon" />
          相关链接
          <HelpTip text="项目实际用到的框架、组件库与工具。点击后由系统默认浏览器打开。" />
        </span>
      </template>
      <template #text>
        <mc-list @change="onLinkChange">
          <mc-list-item
            v-for="l in links"
            :key="l.url"
            :label="l.label"
            :subtitle="l.hint"
            :value="l.url"
            :icon="l.icon"
            icon-right="mc-external-link"
          />
        </mc-list>
      </template>
    </mc-card>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
}

.brand {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 16px;
}

.brand__logo {
  width: 96px;
  height: 96px;
  object-fit: contain;
  flex: 0 0 auto;
}

.brand__meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.brand__name {
  font-family: "Minecraft Ten", system-ui, sans-serif;
  font-size: 22px;
  letter-spacing: 0.02em;
}

.brand__tagline {
  font-size: 12px;
  opacity: 0.6;
}

.author {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 16px;
}

.author--skeleton {
  margin-bottom: 8px;
}

.author__lines {
  margin-bottom: 16px;
}

.author__avatar {
  width: 72px;
  height: 72px;
  flex: 0 0 auto;
  border-radius: 4px;
  /* 头像是放大过的像素画，平滑插值会糊 */
  image-rendering: pixelated;
  background: #1b1b1e;
}

.author__avatar--empty {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-family: "Minecraft Ten", system-ui, sans-serif;
  font-size: 32px;
  opacity: 0.5;
}

.author__meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.author__ids {
  font-size: 12px;
  opacity: 0.6;
}

.author__links {
  margin-top: 16px;
}

.card-title {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.card-icon {
  opacity: 0.7;
}

.lead {
  font-size: 13px;
  line-height: 1.75;
  opacity: 0.85;
  margin: 0 0 16px;
}

.facts {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.fact {
  display: grid;
  grid-template-columns: 96px 1fr;
  align-items: baseline;
  gap: 12px;
}

.fact-key {
  font-size: 12px;
  opacity: 0.55;
}

.fact-val {
  font-size: 13px;
}

.mono {
  font-family: Consolas, "Cascadia Mono", monospace;
}

/* 尾图标由 McList 用 icon-right 渲染在子组件内部，要用 :deep 才能选中 */
:deep(.mc-list__item-icon--right) {
  opacity: 0.45;
}
</style>
