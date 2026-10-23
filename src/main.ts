import { createApp } from "vue";
import { createMcUI } from "mcui-oreui";
import { mcNormalIconSet } from "mcui-oreui/icons/normal";
import App from "./App.vue";
import "mcui-oreui/styles/fonts.css";
import "./styles.css";

const mcui = createMcUI({
  icons: { sets: { mc: mcNormalIconSet } },
  sounds: { enabled: false },
  theme: {
    defaultTheme: "ore",
  },
  locale: {
    locale: "zh-CN",
    fallback: "en",
  },
});

createApp(App).use(mcui).mount("#app");
