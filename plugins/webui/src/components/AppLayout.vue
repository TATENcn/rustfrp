<script setup lang="ts">
import { ref, computed, h, watch } from "vue";
import { useRouter, useRoute } from "vue-router";
import {
  NLayout,
  NLayoutHeader,
  NLayoutSider,
  NLayoutContent,
  NMenu,
  NButton,
  NSpace,
  NSwitch,
  NIcon,
  useMessage,
  type MenuOption,
} from "naive-ui";
import { useI18n } from "@/i18n";
import { useSystemStore } from "@/stores/system";
import { onMounted } from "vue";

const router = useRouter();
const route = useRoute();
const i18n = useI18n();
const message = useMessage();
const systemStore = useSystemStore();

const collapsed = ref(false);
const hasToken = ref(!!localStorage.getItem("api_token"));

// ---- i18n label (computed for reliable template reactivity) ---------------
const localeLabel = computed(() =>
  i18n.locale.value === "zh" ? "EN" : "中文",
);

// ---- sidebar icon renderers (inline SVG, no extra deps) -------------------

function renderIcon(type: string) {
  return () => {
    const paths: Record<string, string> = {
      dashboard:
        "M3 13h8V3H3v10zm0 8h8v-6H3v6zm10 0h8V11h-8v10zm0-18v6h8V3h-8z",
      profiles:
        "M20 3H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 14H4V5h16v12zM6 7h5v5H6V7z",
      proxies:
        "M3.9 12c0-1.71 1.39-3.1 3.1-3.1h4V7H7c-2.76 0-5 2.24-5 5s2.24 5 5 5h4v-1.9H7c-1.71 0-3.1-1.39-3.1-3.1zM8 13h8v-2H8v2zm9-6h-4v1.9h4c1.71 0 3.1 1.39 3.1 3.1s-1.39 3.1-3.1 3.1h-4V17h4c2.76 0 5-2.24 5-5s-2.24-5-5-5z",
      bindings:
        "M16 6l-2.59 2.59L8.41 3.59 3.82 8.18l5 5 1.59-1.59L16 6zm-7.18 2.18l2.59 2.59-1.59 1.59-5-5 4-4 1.59 1.59-2.59 2.59zM19.41 7l-1.59-1.59L16 7.18l1.59 1.59L19.41 7z",
      visitors:
        "M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5s-3 1.34-3 3 1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z",
      logs: "M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5V5h14v14zM7 7h10v2H7V7zm0 4h10v2H7v-2zm0 4h7v2H7v-2z",
      status:
        "M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM9 17H7v-7h2v7zm4 0h-2V7h2v10zm4 0h-2v-4h2v4z",
    };
    return h(NIcon, null, () =>
      h(
        "svg",
        {
          viewBox: "0 0 24 24",
          width: "20",
          height: "20",
          fill: "currentColor",
        },
        h("path", { d: paths[type] ?? "" }),
      ),
    );
  };
}

const menuOptions = computed<MenuOption[]>(() => [
  {
    label: i18n.t("nav.dashboard"),
    key: "dashboard",
    icon: renderIcon("dashboard"),
  },
  {
    label: i18n.t("nav.profiles"),
    key: "profiles",
    icon: renderIcon("profiles"),
  },
  { label: i18n.t("nav.proxies"), key: "proxies", icon: renderIcon("proxies") },
  {
    label: i18n.t("nav.bindings"),
    key: "bindings",
    icon: renderIcon("bindings"),
  },
  {
    label: i18n.t("nav.visitors"),
    key: "visitors",
    icon: renderIcon("visitors"),
  },
  { label: i18n.t("nav.logs"), key: "logs", icon: renderIcon("logs") },
  { label: i18n.t("nav.status"), key: "status", icon: renderIcon("status") },
]);

// Derive menu key from route path (path is always defined, unlike name which
// can be undefined during async lazy-load navigation).
function pathToKey(p: string): string {
  if (p.startsWith("/proxies")) return "proxies";
  if (p.startsWith("/profiles")) return "profiles";
  if (p.startsWith("/bindings")) return "bindings";
  if (p.startsWith("/visitors")) return "visitors";
  if (p.startsWith("/logs")) return "logs";
  if (p.startsWith("/status")) return "status";
  return "dashboard";
}

// Use a local ref so we can update it IMMEDIATELY on click, before the async
// route resolution completes.  Without this, the :value binding races with the
// lazy chunk load and the menu snaps back to the previous selection.
const menuKey = ref(pathToKey(route.path));

// Sync from route changes (covers browser back/forward, direct URL navigation)
watch(
  () => route.path,
  (p) => {
    menuKey.value = pathToKey(p);
  },
);

function handleMenuClick(key: string) {
  menuKey.value = key;
  router.push({ name: key });
}

const isDark = ref(localStorage.getItem("rustfrp-theme") === "dark");

function toggleTheme(val: boolean) {
  isDark.value = val;
  localStorage.setItem("rustfrp-theme", val ? "dark" : "light");
}

function toggleLocale() {
  const next = i18n.locale.value === "zh" ? "en" : "zh";
  i18n.setLocale(next);
}

async function handleReload() {
  try {
    const result = await systemStore.triggerReload();
    if (result) {
      message.success(`Reload started: ${result}`);
    }
  } catch {
    message.error(i18n.t("error.serverError"));
  }
}

async function handleLogout() {
  localStorage.removeItem("api_token");
  router.push({ name: "login" });
}

const statusText = computed(() => {
  const count = systemStore.status?.active_frpc_instances ?? 0;
  return i18n.t("status.frpcRunning", { count });
});

onMounted(() => {
  systemStore.fetchStatus();
});
</script>

<template>
  <NLayout style="height: 100vh; display: flex; flex-direction: column">
    <!-- Header -->
    <NLayoutHeader
      bordered
      style="height: 48px; flex-shrink: 0; padding: 0 16px"
    >
      <div
        style="
          display: flex;
          align-items: center;
          justify-content: space-between;
          height: 100%;
        "
      >
        <span style="font-weight: 600; font-size: 16px">{{
          i18n.t("app.title")
        }}</span>
        <NSpace align="center" :size="8">
          <!-- i18n toggle — primary button so it's clearly visible -->
          <NButton
            type="primary"
            size="small"
            style="font-weight: 500"
            @click="toggleLocale"
          >
            🌐 {{ localeLabel }}
          </NButton>
          <NSwitch :value="isDark" @update:value="toggleTheme" size="small">
            <template #checked>🌙</template>
            <template #unchecked>☀️</template>
          </NSwitch>
          <NButton text size="small" @click="handleReload">
            {{ i18n.t("app.reload") }}
          </NButton>
          <NButton v-if="hasToken" text size="small" @click="handleLogout">
            {{ i18n.t("auth.logout") }}
          </NButton>
        </NSpace>
      </div>
    </NLayoutHeader>

    <!-- Body: sidebar + content -->
    <NLayout :has-sider="true" style="flex: 1; min-height: 0">
      <NLayoutSider
        bordered
        :collapsed="collapsed"
        collapse-mode="width"
        :width="200"
        :collapsed-width="64"
        show-trigger
        @collapse="collapsed = true"
        @expand="collapsed = false"
      >
        <NMenu
          :value="menuKey"
          :options="menuOptions"
          :collapsed="collapsed"
          :collapsed-width="64"
          @update:value="handleMenuClick"
        />
      </NLayoutSider>

      <!-- Content: pb-32px ensures last items aren't hidden behind fixed footer -->
      <NLayoutContent
        content-style="padding: 16px; padding-bottom: 40px; overflow-y: auto; height: 100%; box-sizing: border-box"
      >
        <RouterView />
      </NLayoutContent>
    </NLayout>

    <!-- Footer — fixed to viewport bottom, never scrolls -->
    <div
      style="
        position: fixed;
        bottom: 0;
        left: 0;
        right: 0;
        height: 32px;
        z-index: 10;
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0 16px;
        border-top: 1px solid var(--n-border-color);
        background: var(--n-color);
      "
    >
      <span style="font-size: 12px; color: var(--n-text-color-3)">
        {{ i18n.t("app.ready") }} · {{ statusText }}
        <template v-if="systemStore.status">
          · uptime {{ Math.floor(systemStore.status.uptime_secs / 3600) }}h
          {{ Math.floor((systemStore.status.uptime_secs % 3600) / 60) }}m
        </template>
      </span>
      <span style="font-size: 11px; color: var(--n-text-color-3)">
        RustFRP v0.1.0
      </span>
    </div>
  </NLayout>
</template>
