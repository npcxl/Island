<script>
  import { onMount, onDestroy } from "svelte";
  import Icon from "@iconify/svelte";

  let battery = $state({ percent: 100, charging: false });
  let visible  = $state(false); // 入场动画控制
  let unlisten = null;

  onMount(async () => {
    try {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen("battery-change", (e) => {
        battery = e.payload;
        // 每次收到电池事件，如果在充电就触发入场动画
        if (e.payload.charging && !visible) {
          requestAnimationFrame(() => requestAnimationFrame(() => { visible = true; }));
        }
      });
    } catch {}
  });
  onDestroy(() => { unlisten?.(); });

  let batteryColor = $derived(
    battery.percent > 40 ? "#34c759"
    : battery.percent > 15 ? "#ff9f0a"
    : "#ff3b5c"
  );

  const CIRC = 138.2; // 2π×22
  let dashFill = $derived((battery.percent / 100) * CIRC);
</script>

<!--
  副岛内容。
  注意：窗口的显示/隐藏由 Rust 的 SetWindowPos 控制。
  这里只负责内容动画（入场弹出效果）。
  html/body 在 sub.html 里已设为透明，这里不显示任何背景。
-->
<div class="sub-wrap" class:visible>
  <div class="blur-bg"></div>

  <svg class="ring" viewBox="0 0 60 60">
    <circle class="ring-track" cx="30" cy="30" r="22" />
    <circle
      class="ring-fill"
      cx="30" cy="30" r="22"
      stroke={batteryColor}
      stroke-dasharray="{dashFill} {CIRC}"
    />
  </svg>

  <div class="center">
    <div class="bolt-wrap">
      <Icon icon="solar:bolt-bold" width="15" color={batteryColor} />
    </div>
    <span class="pct" style:color={batteryColor}>{battery.percent}%</span>
  </div>
</div>

<style>
  :global(html), :global(body), :global(#sub-app) {
    margin: 0; padding: 0;
    width: 100%; height: 100%;
    background: transparent !important;
    overflow: hidden;
  }

  /* 分裂入场：从右侧（主岛方向）以水滴形扩张弹出 */
  .sub-wrap {
    width: 60px; height: 60px;
    border-radius: 50%;
    position: relative;
    display: flex; align-items: center; justify-content: center;

    transform-origin: right center;
    transform: scaleX(0) scaleY(0.5);
    opacity: 0;
    transition:
      transform 0.5s cubic-bezier(0.34, 1.56, 0.64, 1),
      opacity   0.22s ease,
      border-radius 0.45s cubic-bezier(0.34, 1.56, 0.64, 1);
    border-radius: 80% 20% 80% 20% / 50% 50% 50% 50%;
  }
  .sub-wrap.visible {
    transform: scaleX(1) scaleY(1);
    opacity: 1;
    border-radius: 50%;
  }

  .blur-bg {
    position: absolute; inset: 0; border-radius: inherit;
    background: rgba(10,10,10,0.88);
    backdrop-filter: blur(20px) saturate(1.6);
    -webkit-backdrop-filter: blur(20px) saturate(1.6);
    border: 1px solid rgba(255,255,255,0.08);
    box-shadow:
      0 6px 24px rgba(0,0,0,0.55),
      0 0 18px 2px rgba(52,199,89,0.22);
  }

  .ring {
    position: absolute; inset: 3px;
    width: calc(100% - 6px); height: calc(100% - 6px);
    transform: rotate(-90deg);
  }
  .ring-track { fill: none; stroke: rgba(255,255,255,0.08); stroke-width: 3; }
  .ring-fill {
    fill: none; stroke-width: 3; stroke-linecap: round;
    transition: stroke-dasharray 0.9s ease, stroke 0.4s ease;
    animation: ring-pulse 1.8s ease-in-out infinite alternate;
  }
  @keyframes ring-pulse {
    from { opacity: 0.85; filter: drop-shadow(0 0 3px currentColor); }
    to   { opacity: 1;    filter: drop-shadow(0 0 8px currentColor); }
  }

  .center {
    position: relative; z-index: 1;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 1px;
  }
  .bolt-wrap {
    line-height: 1; display: flex;
    animation: bolt-pulse 1.1s ease-in-out infinite;
  }
  @keyframes bolt-pulse {
    0%, 100% { opacity: 1;    transform: scale(1);    }
    50%       { opacity: 0.45; transform: scale(0.82); }
  }
  .pct { font-size: 9px; font-weight: 800; line-height: 1; letter-spacing: -0.3px; }
</style>
