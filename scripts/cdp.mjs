/**
 * CDP 调试客户端：连上 Tauri WebView，执行 JS 并打印结果。
 *
 * 用来在不用鼠标点、不用截图猜的情况下验证 UI 行为。
 * 端口由 MCSPEED_DEVTOOLS_PORT 决定，默认 9333。
 *
 *   node scripts/cdp.mjs "document.querySelectorAll('.mc-card').length"
 *   node scripts/cdp.mjs --click "总览"        # 点侧边栏某一项
 *   node scripts/cdp.mjs --dom                # dump 可交互元素清单
 */
import WebSocket from "ws";

const PORT = process.env.MCSPEED_DEVTOOLS_PORT ?? "9333";
const args = process.argv.slice(2);

const flagIndex = args.findIndex((a) => a.startsWith("--"));
const mode = flagIndex >= 0 ? args[flagIndex] : "--eval";
const payload = flagIndex >= 0 ? args[flagIndex + 1] : args[0];

/** 列出所有 target，取第一个 page。 */
async function findPageWs() {
  const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
  const targets = await res.json();
  const page = targets.find((t) => t.type === "page");
  if (!page) throw new Error(`没有 page target，只有：${targets.map((t) => t.type).join(", ")}`);
  return page.webSocketDebuggerUrl;
}

/** 连上去，按序发送命令，收齐结果后退出。 */
async function withCdp(fn) {
  const wsUrl = await findPageWs();
  const ws = new WebSocket(wsUrl, { perMessageDeflate: false });
  let nextId = 1;
  const pending = new Map();

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });

  ws.on("message", (raw) => {
    const msg = JSON.parse(raw.toString());
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(`${msg.error.message} (${msg.error.code})`));
      else resolve(msg.result);
    }
  });

  await new Promise((resolve, reject) => {
    ws.once("open", resolve);
    ws.once("error", reject);
  });

  try {
    return await fn(send);
  } finally {
    ws.close();
  }
}

/** Runtime.evaluate，自动等表达式返回。 */
async function evaluate(send, expression) {
  const result = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });

  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description ?? "JS 执行出错");
  }
  return result.result.value;
}

switch (mode) {
  case "--eval": {
    const value = await withCdp((send) => evaluate(send, payload));
    console.log(typeof value === "object" ? JSON.stringify(value, null, 2) : String(value));
    break;
  }

  case "--dom": {
    const value = await withCdp((send) =>
      evaluate(send, `
        JSON.stringify([...document.querySelectorAll('button,[role="switch"],[role="tab"],input')]
          .map((el, i) => ({
            i,
            tag: el.tagName.toLowerCase(),
            role: el.getAttribute('role'),
            text: (el.innerText || el.getAttribute('aria-label') || '').trim().slice(0, 40),
            disabled: el.disabled || el.getAttribute('aria-disabled') === 'true',
          })), null, 2)
      `)
    );
    console.log(value);
    break;
  }

  case "--click": {
    // 按可见文本找元素并点下去。返回是否命中。
    const value = await withCdp((send) =>
      evaluate(send, `
        (() => {
          const want = ${JSON.stringify(payload)};
          const el = [...document.querySelectorAll('button,[role="switch"],[role="tab"],.mc-list__item,label')]
            .find((e) => (e.innerText || '').trim().includes(want));
          if (!el) return 'NOT FOUND';
          el.scrollIntoView({ block: 'center' });
          el.click();
          return 'CLICKED: ' + (el.innerText || '').trim().slice(0, 40);
        })()
      `)
    );
    console.log(value);
    break;
  }

  case "--switch": {
    // 读某个开关的当前状态
    const value = await withCdp((send) =>
      evaluate(send, `
        (() => {
          const el = document.querySelector('[role="switch"]');
          if (!el) return 'no switch found';
          return JSON.stringify({
            checked: el.checked,
            disabled: el.disabled,
            label: el.getAttribute('aria-label'),
          });
        })()
      `)
    );
    console.log(value);
    break;
  }

  default:
    console.error(`未知模式 ${mode}`);
    process.exit(1);
}
