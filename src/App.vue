<script setup lang="ts">
import { onMounted, ref } from "vue";
import AppDialog from "./components/AppDialog.vue";
import SideNav from "./components/SideNav.vue";
import TitleBar from "./components/TitleBar.vue";
import OverviewPage from "./pages/OverviewPage.vue";
import DomainsPage from "./pages/DomainsPage.vue";
import HostsPage from "./pages/HostsPage.vue";
import DiagnosticsPage from "./pages/DiagnosticsPage.vue";
import AboutPage from "./pages/AboutPage.vue";
import { getStatus } from "./api";
import type { ProxyStatus } from "./types";

const status = ref<ProxyStatus | null>(null);
const active = ref("overview");

onMounted(() => {
  void refresh();
});

async function refresh() {
  try {
    status.value = await getStatus();
  } catch {
    status.value = null;
  }
}
</script>

<template>
  <mc-app>
    <!-- 标题栏跨满整行 -->
    <TitleBar />

    <!-- 侧边栏 + 内容区 -->
    <div class="shell">
      <SideNav v-model="active" />

      <main class="shell__content">
        <mc-scroll-view>
          <mc-container>
            <!--
              页面切换用 out-in：先淡出旧页再淡入新页。
              默认的同时进出会让两个页叠在文档流里，容器高度会跳。
            -->
            <Transition name="page" mode="out-in">
              <OverviewPage v-if="active === 'overview'" :status="status" @refresh="refresh" />
              <DomainsPage v-else-if="active === 'domains'" :status="status" @refresh="refresh" />
              <HostsPage v-else-if="active === 'hosts'" :status="status" @refresh="refresh" />
              <DiagnosticsPage v-else-if="active === 'diagnostics'" :status="status" />
              <AboutPage v-else-if="active === 'about'" />
            </Transition>
          </mc-container>
        </mc-scroll-view>
      </main>
    </div>

    <!-- 全局信息框 -->
    <AppDialog />
  </mc-app>
</template>

<style scoped>
.shell {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: row;
  width: 100%;
}

.shell__content {
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

/* 页面切换：淡出上飘 / 淡入上移，0.15s 收完，不拖泥带水 */
.page-enter-active,
.page-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}

.page-enter-from {
  opacity: 0;
  transform: translateY(8px);
}

.page-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

/* 系统开了「减少动态效果」就直接切，不折腾 */
@media (prefers-reduced-motion: reduce) {
  .page-enter-active,
  .page-leave-active {
    transition: none;
  }
}
</style>
