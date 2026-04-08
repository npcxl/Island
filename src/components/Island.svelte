<script>
  import { onMount, onDestroy } from "svelte";
  import { spring } from "svelte/motion";
  import { cubicOut } from "svelte/easing";
  import Icon from "@iconify/svelte";
  import MediaCover from "./MediaCover.svelte";
  import ChromeFollowBar from "./ChromeFollowBar.svelte";

  // ── 核心状态 ─────────────────────────────────────────────
  let media = $state({ title: "", artist: "", thumbnail: "", accent: "#a0a0a0", status: "stopped", position_ms: 0, duration_ms: 0, source: "unknown" });
  let battery = $state({ percent: 100, charging: false });
  let hasMedia = $derived(media.title.length > 0);
  /** 后端能识别来源时才显示「打开播放器」 */
  let canOpenPlayer = $derived(
    hasMedia && media.source && media.source !== "unknown"
  );
  let lyrics = $state(null); // null | { title, artist, plain?, synced?, source }
  let coverUrl = $state(""); // 网易云高清封面 URL（B：优先 URL）
  // ── 前端健康检查（用于定位“前端失效”）────────────────────
  let dbg = $state({ boot: "init", lastMediaAt: 0, lastEvent: "", err: "" });

  /**
   * mode（持久模式）:
   *   "idle"     — 无媒体无充电
   *   "charging" — 仅充电
   *   "media"    — 有音乐（不区分是否充电）
   *   "expanded" — 点击展开
   *
   * overlay（瞬时覆盖层，不改变 mode）:
   *   null | { type: "VolumeChange"|"CapsLock"|"LowBattery"|"WeChat"|"AppToast", ... }
   */
  let mode    = $state("idle");
  let overlay = $state(null);

  let pressed        = $state(false);
  let hovered        = $state(false);
  let contentOpacity = $state(0);
  let overlayOpacity = $state(0);

  // ── 弹簧尺寸（主岛）─────────────────────────────────────
  const w = spring(120, { stiffness: 0.18, damping: 0.72 });
  const h = spring(34,  { stiffness: 0.18, damping: 0.72 });
  const r = spring(999, { stiffness: 0.18, damping: 0.72 });

  // ── 尺寸表 ───────────────────────────────────────────────
  const SIZES = {
    idle:       { w: 100, h: 32,  r: 999 },
    charging:   { w: 90,  h: 32,  r: 999 },
    media:      { w: 280, h: 38,  r: 20  },
    /** Chrome 前台跟随（与媒体条同高风格） */
    chrome_focus: { w: 320, h: 72, r: 20 },
    expanded:   { w: 360, h: 312, r: 20  },
    ov_volume:  { w: 220, h: 40,  r: 999 },
    ov_caps:    { w: 140, h: 38,  r: 999 },
    ov_lowbat:    { w: 180, h: 44,  r: 999 },
    ov_wechat:    { w: 260, h: 40,  r: 999 },
    /** 系统 Toast：两行文案，高度约为单行 overlay 的 2 倍 */
    ov_app_toast: { w: 300, h: 80,  r: 22 },
  };

  /**
   * 系统 Toast：按 AUMID / 显示名匹配常用应用 Iconify 图标（无系统 Logo 时）。
   * 格式：[子串, iconify id, 可选色] — 越靠前越优先。
   */
  const APP_TOAST_ICON_RULES = [
    ["anysphere", "vscode-icons:file-type-cursor", null],
    ["cursor", "vscode-icons:file-type-cursor", null],
    ["microsoft teams", "simple-icons:microsoftteams", "#6264A7"],
    ["visual studio code", "simple-icons:visualstudiocode", "#007ACC"],
    ["vscode", "simple-icons:visualstudiocode", "#007ACC"],
    ["outlook", "simple-icons:microsoftoutlook", "#0078D4"],
    ["onedrive", "simple-icons:microsoftonedrive", "#0078D4"],
    ["neteasemusic", "simple-icons:neteasemusic", "#E60026"],
    ["cloudmusic", "simple-icons:neteasemusic", "#E60026"],
    ["whatsapp", "simple-icons:whatsapp", "#25D366"],
    ["telegram", "simple-icons:telegram", "#26A5E4"],
    ["discord", "simple-icons:discord", "#5865F2"],
    ["slack", "simple-icons:slack", "#E01E5A"],
    ["zoom", "simple-icons:zoom", "#2D8CFF"],
    ["skype", "simple-icons:skype", "#00AFF0"],
    ["spotify", "simple-icons:spotify", "#1DB954"],
    ["chrome", "simple-icons:googlechrome"],
    ["msedge", "simple-icons:microsoftedge"],
    ["microsoft edge", "simple-icons:microsoftedge"],
    ["firefox", "simple-icons:firefox", "#FF7139"],
    ["steam", "simple-icons:steam", "#B8C5D0"],
    ["epicgames", "simple-icons:epicgames"],
    ["钉钉", "simple-icons:dingtalk", "#0089FF"],
    ["dingtalk", "simple-icons:dingtalk", "#0089FF"],
    ["飞书", "simple-icons:feishu", "#3370FF"],
    ["feishu", "simple-icons:feishu", "#3370FF"],
    ["lark", "simple-icons:lark", "#3370FF"],
    ["哔哩哔哩", "simple-icons:bilibili", "#00A1D6"],
    ["bilibili", "simple-icons:bilibili", "#00A1D6"],
    ["网易云音乐", "simple-icons:neteasemusic", "#E60026"],
    ["网易云", "simple-icons:neteasemusic", "#E60026"],
    ["酷狗音乐", "mdi:music-circle", "#1296DB"],
    ["酷狗", "mdi:music-circle", "#1296DB"],
    ["kugou", "mdi:music-circle", "#1296DB"],
    ["酷我", "mdi:music", "#FB0C28"],
    ["微博", "simple-icons:sinaweibo", "#E6162D"],
    ["sinaweibo", "simple-icons:sinaweibo", "#E6162D"],
    ["抖音", "simple-icons:tiktok"],
    ["tiktok", "simple-icons:tiktok"],
    ["tim", "simple-icons:tencentqq", "#12B7F5"],
    ["qq", "simple-icons:tencentqq", "#12B7F5"],
    ["企业微信", "simple-icons:wechat", "#07C160"],
    ["wxwork", "simple-icons:wechat", "#07C160"],
    ["微信", "simple-icons:wechat", "#07C160"],
    ["wechat", "simple-icons:wechat", "#07C160"],
    ["支付宝", "simple-icons:alipay", "#1677FF"],
    ["alipay", "simple-icons:alipay", "#1677FF"],
    ["淘宝", "simple-icons:taobao", "#FF5000"],
    ["taobao", "simple-icons:taobao", "#FF5000"],
    ["小红书", "simple-icons:xiaohongshu", "#FF2442"],
    ["xiaohongshu", "simple-icons:xiaohongshu", "#FF2442"],
    ["apple music", "simple-icons:applemusic", "#FA243C"],
    ["notion", "simple-icons:notion"],
    ["obsidian", "simple-icons:obsidian", "#7C3AED"],
    ["figma", "simple-icons:figma"],
    ["telegramdesktop", "simple-icons:telegram", "#26A5E4"],
    ["mail", "mdi:email-outline"],
    ["邮件", "mdi:email-outline"],
    ["日历", "mdi:calendar"],
    ["calendar", "mdi:calendar"],
    ["照片", "mdi:image-multiple"],
    ["photos", "mdi:image-multiple"],
  ];

  function resolveAppToastIcon(aumid, appName) {
    const blob = `${aumid ?? ""} ${appName ?? ""}`.trim();
    const lo = blob.toLowerCase();
    for (const row of APP_TOAST_ICON_RULES) {
      const needle = row[0];
      const icon = row[1];
      const colorThird = row[2];
      const nlo = needle.toLowerCase();
      if (lo.includes(nlo) || blob.includes(needle)) {
        if (row.length >= 3) {
          return { icon, color: colorThird === null ? null : colorThird };
        }
        return { icon, color: "rgba(255,255,255,0.88)" };
      }
    }
    /* 多色 SVG，勿强行单色 */
    return { icon: "ic:round-apps", color: "rgba(255,255,255,0.9)" };
  }

  function applySize(key) { const s = SIZES[key]; w.set(s.w); h.set(s.h); r.set(s.r); }

  function switchMode(next) {
    contentOpacity = 0;
    if (next !== "expanded" && mode !== "expanded") lastModeBeforeExpanded = next;
    mode = next;
    applySize(next);

    setTimeout(() => (contentOpacity = 1), 180);
  }

  // ── overlay 管理 ────────────────────────────────────────
  let _overlayTimer = null;
  let _lastAppToastKey = "";
  let _lastAppToastAt = 0;

  function showOverlay(ev) {
    if (ev.type === "AppToast") {
      const key = `${ev.app_name}\n${ev.aumid ?? ""}\n${ev.title}\n${ev.body}`;
      const now = Date.now();
      if (key === _lastAppToastKey && now - _lastAppToastAt < 5200) return;
      _lastAppToastKey = key;
      _lastAppToastAt = now;
    }

    clearTimeout(_overlayTimer);
    overlayOpacity = 0;
    overlay = ev;

    const sizeKey = ev.type === "VolumeChange" ? "ov_volume"
                  : ev.type === "CapsLock"      ? "ov_caps"
                  : ev.type === "WeChat"        ? "ov_wechat"
                  : ev.type === "AppToast"     ? "ov_app_toast"
                  : "ov_lowbat";
    applySize(sizeKey);
    requestAnimationFrame(() => { overlayOpacity = 1; });

    const dur = ev.type === "CapsLock"    ? 1500
              : ev.type === "LowBattery"  ? 3000
              : ev.type === "WeChat"      ? 4500
              : ev.type === "AppToast"    ? 5000
              : 2200;

    const fadeOutMs = ev.type === "AppToast" ? 260 : 200;

    _overlayTimer = setTimeout(() => {
      overlayOpacity = 0;
      setTimeout(() => {
        overlay = null;
        applySize(mode);
      }, fadeOutMs);
    }, dur);
  }

  function handleSystemEvent(ev) { showOverlay(ev); }

  // ── 模式决策 ────────────────────────────────────────────
  /** @type {null | { page_title: string, url?: string | null, has_sessions: boolean, audible: boolean, chrome_muted: boolean }} */
  let chromeBar = $state(null);

  function resolveMode() {
    if (mode === "expanded") return;
    // Chrome 在前台时优先显示浏览器条；否则只要 SMTC 有歌就会一直占 media，用户在 Chrome 里永远看不到跟随条
    if (chromeBar)                           { if (mode !== "chrome_focus") switchMode("chrome_focus"); }
    else if (hasMedia)                       { if (mode !== "media")        switchMode("media");        }
    else if (battery.charging)              { if (mode !== "charging")     switchMode("charging");     }
    else                                    { if (mode !== "idle")         switchMode("idle");         }
  }

  function handleChromeFocus(payload) {
    chromeBar = payload ?? null;
    resolveMode();
    const u = String(payload?.url ?? "").trim();
    if (!u) bookmarkPageSaved = false;
    else invokeSafe("bookmark_exists", { url: u }).then((ex) => { bookmarkPageSaved = !!ex; });
  }

  /** 后端 SMTC（尤其网易云）常把暂停仍报成 playing；点击播放/暂停时写入覆盖，直到切歌或与后端一致 */
  let playStateOverride = $state(/** @type {boolean | null} */ (null));

  function normalizedPlaying(status) {
    return String(status ?? "").toLowerCase() === "playing";
  }

  function handleMediaChange(payload) {
    const next = payload ?? { title: "", artist: "", thumbnail: "", accent: "#a0a0a0", status: "stopped", position_ms: 0, duration_ms: 0, source: "unknown" };
    if (playStateOverride !== null) {
      const sameTrack =
        (next.title ?? "") === (media.title ?? "") && (next.artist ?? "") === (media.artist ?? "");
      if (!sameTrack) {
        playStateOverride = null;
      } else if (normalizedPlaying(next.status) === playStateOverride) {
        playStateOverride = null;
      }
    }
    media = next;
    console.log("[media]", media.title, "thumbnail:", media.thumbnail ? "有图片(" + media.thumbnail.length + "字符)" : "无图片", "source:", media.source);
    dbg.lastMediaAt = Date.now();
    dbg.lastEvent = `media-change:${media.title || "<empty>"}`;
    resolveMode();
  }

  function parseSyncedLyrics(synced) {
    if (!synced || !synced.trim()) return [];
    const out = [];
    for (const raw of synced.split(/\r?\n/)) {
      const line = raw.trim();
      if (!line) continue;
      // 支持一行多个时间戳： [00:10.00][00:12.00]text
      const tags = [...line.matchAll(/\[(\d{1,2}):(\d{2})(?:\.(\d{1,3}))?\]/g)];
      if (!tags.length) continue;
      const text = line.replace(/\[[^\]]+\]/g, "").trim();
      if (!text) continue;
      for (const m of tags) {
        const mm = Number(m[1]);
        const ss = Number(m[2]);
        const frac = (m[3] ?? "0").padEnd(3, "0").slice(0, 3);
        const ms = Number(frac);
        out.push({ t: mm * 60_000 + ss * 1000 + ms, text });
      }
    }
    out.sort((a, b) => a.t - b.t);
    const dedup = [];
    for (const it of out) {
      const last = dedup[dedup.length - 1];
      if (last && last.t === it.t) last.text = it.text;
      else dedup.push({ ...it });
    }
    console.log("[lyrics] parsed:", dedup.length, "lines");
    return dedup;
  }

  function findLyricIndex(lines, posMs) {
    if (!lines?.length) return -1;
    const t = Math.max(0, Number(posMs ?? 0));
    // 二分：找 <= t 的最大索引
    let lo = 0, hi = lines.length - 1, ans = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (lines[mid].t <= t) { ans = mid; lo = mid + 1; }
      else { hi = mid - 1; }
    }
    return ans < 0 ? 0 : ans;
  }
  function handleBatteryChange(payload) {
    console.log("[battery]", payload);
    battery = payload;
    resolveMode();
  }

  function firstLyricLine(lrc) {
    if (!lrc) return "";
    const synced = lrc.synced ?? "";
    if (synced.trim()) {
      // [mm:ss.xx] text
      const lines = synced.split(/\r?\n/);
      for (const line of lines) {
        const t = line.replace(/^\s*\[[^\]]+\]\s*/, "").trim();
        if (t) return t;
      }
    }
    const plain = lrc.plain ?? "";
    if (plain.trim()) {
      const lines = plain.split(/\r?\n/);
      for (const line of lines) {
        const t = line.trim();
        if (t) return t;
      }
    }
    return "";
  }

  function platformIcon(source) {
    const s = (source || "").toLowerCase();
    if (s.includes("cloudmusic") || s.includes("netease")) return "simple-icons:neteasecloudmusic";
    if (s.includes("qqmusic")) return "simple-icons:qqmusic";
    if (s.includes("spotify")) return "simple-icons:spotify";
    if (s.includes("chrome")) return "simple-icons:googlechrome";
    return null; // 返回 null 表示没有匹配的平台
  }

  // ── 歌词播放时间（纯本地计时）──────────────────────────────
  let playheadMs = $state(0);
  let _phTimer = null;
  let _lastTrackKey = "";
  
  let isPlayingUi = $derived(
    playStateOverride !== null ? playStateOverride : normalizedPlaying(media.status)
  );

  // 播放时间：跟 isPlayingUi（含乐观覆盖），避免 SMTC 撒谎时频谱仍动
  $effect(() => {
    const trackKey = `${media.title ?? ""}||${media.artist ?? ""}`;
    const playing = isPlayingUi;

    if (_phTimer) clearInterval(_phTimer);

    if (trackKey !== _lastTrackKey) {
      _lastTrackKey = trackKey;
      playheadMs = Math.max(0, Number(media.position_ms ?? 0));
    }

    if (playing) {
      let lastTick = Date.now();
      let carried = playheadMs;
      _phTimer = setInterval(() => {
        const now = Date.now();
        carried += now - lastTick;
        lastTick = now;
        playheadMs = carried;
      }, 100);
    } else {
      playheadMs = Math.max(0, Number(media.position_ms ?? 0));
    }

    return () => { if (_phTimer) clearInterval(_phTimer); };
  });

  let lyricLines = $derived(getLyricLines(lyrics));
  let curLyricIndex = $derived(findLyricIndex(lyricLines, playheadMs));
  let curLyric = $derived(curLyricIndex >= 0 ? (lyricLines[curLyricIndex]?.text ?? "") : firstLyricLine(lyrics));
  
  // ── 歌词列表（用于滚动显示）───────────────────────────────
  function getLyricLines(lrc) {
    if (!lrc) return [];
    console.log("[lyrics] processing:", lrc.title, "-", lrc.artist);
    const synced = parseSyncedLyrics(lrc.synced ?? "");
    if (synced.length) {
      console.log("[lyrics] using synced lyrics:", synced.length, "lines");
      return synced;
    }
    // 回退到普通歌词分行
    const plain = lrc.plain ?? "";
    const lines = plain.split(/\r?\n/).filter(l => l.trim()).map((text, i) => ({ t: i * 5000, text }));
    console.log("[lyrics] using plain lyrics:", lines.length, "lines");
    return lines;
  }
  // curLyricIndex 已用二分查找派生

  // ── 交互 ─────────────────────────────────────────────────
  function onClick() {
    if (overlay) {
      clearTimeout(_overlayTimer);
      overlayOpacity = 0;
      setTimeout(() => { overlay = null; applySize(mode); }, 200);
      return;
    }
    pressed = true;
    setTimeout(() => (pressed = false), 160);
    if (mode !== "expanded") {
      lastModeBeforeExpanded = mode;
      expandedPanelPage = "main";
      switchMode("expanded");
      refreshSystemSnapshot();
      return;
    }
    expandedPanelPage = "main";
    switchMode(lastModeBeforeExpanded);
    resolveMode();
  }

  function onHoverEnter() {
    hovered = true;
    if (mode === "idle" && !overlay) { w.set(SIZES.idle.w * 1.06); h.set(SIZES.idle.h * 1.06); }
  }
  function onHoverLeave() {
    hovered = false;
    if (mode === "idle" && !overlay) { w.set(SIZES.idle.w); h.set(SIZES.idle.h); }
  }

  // ── 媒体控制 ─────────────────────────────────────────────
  async function ctrlMedia(action, e) {
    e?.stopPropagation();
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      if (action === "play_pause" && hasMedia) {
        const cur = playStateOverride !== null ? playStateOverride : normalizedPlaying(media.status);
        playStateOverride = !cur;
      }
      await invoke("media_control", { action });
      if (action === "play_pause") {
        for (const ms of [150, 400, 800]) {
          setTimeout(async () => {
            try {
              const s = await invoke("get_current_media");
              if (s) handleMediaChange(s);
            } catch {}
          }, ms);
        }
      }
    } catch {}
  }

  async function openPlayingApp(e) {
    e?.stopPropagation();
    if (!canOpenPlayer) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("open_now_playing_app", { source: media.source });
    } catch {}
  }

  // ── 挂载/卸载 ────────────────────────────────────────────
  let unlistens = [];
  let _mediaPoll = null;
  onMount(async () => {
    applySize("idle");
    setTimeout(() => (contentOpacity = 1), 100);
  

    // 捕获运行时错误（避免“白屏不知道原因”）
    const onErr = (e) => { dbg.err = String(e?.message ?? e); };
    const onRej = (e) => { dbg.err = String(e?.reason ?? e); };
    window.addEventListener("error", onErr);
    window.addEventListener("unhandledrejection", onRej);
    try {
      const { listen } = await import("@tauri-apps/api/event");
   
      unlistens.push(
        await listen("media-change",   (e) => handleMediaChange(e.payload)),
        await listen("battery-change", (e) => handleBatteryChange(e.payload)),
        await listen("system-event",   (e) => handleSystemEvent(e.payload)),
        await listen("lyrics-change",  (e) => { 
          console.log("[lyrics] received:", e.payload ? "yes" : "no");
          lyrics = e.payload ?? null; 
        }),
        await listen("cover-url-change", (e) => {
          const p = e.payload;
          if (!p) { coverUrl = ""; return; }
          const same = (p.title ?? "") === (media.title ?? "") && (p.artist ?? "") === (media.artist ?? "");
          console.log("[cover-url-change]", p.title, "-", p.artist, "=>", p.url, "same?", same);
          if (same) coverUrl = p.url ?? "";
        }),
        await listen("chrome-focus", (e) => handleChromeFocus(e.payload)),
      );
      // 启动兜底：主动拉一次当前媒体，避免错过初始 emit
      const { invoke } = await import("@tauri-apps/api/core");
      const snap = await invoke("get_current_media");
      console.log("[bootstrap] get_current_media =>", snap ? "some" : "none");
      if (snap) handleMediaChange(snap);
      // 传感线程可能 300ms 后才写入缓存：再补拉一次
      setTimeout(async () => {
        try {
          const snap2 = await invoke("get_current_media");
          console.log("[bootstrap] get_current_media (retry) =>", snap2 ? "some" : "none");
          if (snap2) handleMediaChange(snap2);
        } catch {}
      }, 600);

      // 事件偶发丢失/渲染异常时的硬兜底：轮询缓存（不会触碰 WinRT）
      _mediaPoll = setInterval(async () => {
        try {
          const s = await invoke("get_current_media");
          if (!s) return;
          if (
            s.title !== media.title
            || s.artist !== media.artist
            || s.status !== media.status
            || (s.status !== "playing"
              && Math.floor((s.position_ms ?? 0) / 1000) !== Math.floor((media.position_ms ?? 0) / 1000))
          ) {
            handleMediaChange(s);
          }
        } catch {}
      }, 500);
   
    } catch (err) {
      console.error("[bootstrap] failed", err);
   
      dbg.err = String(err?.message ?? err);
    }

    return () => {
      window.removeEventListener("error", onErr);
      window.removeEventListener("unhandledrejection", onRej);
    };
  }); 
  onDestroy(() => {
    unlistens.forEach(u => u());
    clearTimeout(_overlayTimer);
    if (_mediaPoll) clearInterval(_mediaPoll);
  });

  // ── 派生值 ───────────────────────────────────────────────
  let playIcon  = $derived(isPlayingUi ? "solar:pause-circle-bold" : "solar:play-circle-bold");
  // 系统控制面板状态（expanded 时拉取快照）
  let sysAccent = $state(null); // "#RRGGBB" | null
  let sysVolume = $state({ percent: 0, muted: false });
  let sysBrightness = $state({ supported: true, percent: 50 });
  let sysRadios = $state({ wifi: null, bluetooth: null }); // null=未知
  let sysDnd = $state({ supported: true, on: null }); // on: boolean|null
  let sysTheme = $state({ mode: "unknown" }); // "light" | "dark" | "unknown"

  /** 网站收藏（SQLite） */
  let bookmarksList = $state(/** @type {Array<{id:number,url:string,title:string,created_at:number}>} */ ([]));
  /** expanded 子页：主面板 | 收藏列表全屏 */
  let expandedPanelPage = $state(/** @type {"main" | "bookmarks"} */ ("main"));
  let bookmarkPageSaved = $state(false);
  let bookmarkTargetUrl = $derived((chromeBar?.url ?? "").trim());
  let canBookmarkFromChrome = $derived(!!chromeBar && bookmarkTargetUrl.length > 0);

  let _snapTimer = null;
  let _drag = $state(null); // "vol" | "bri" | null
  let jellyKey = $state(0);
  let lastModeBeforeExpanded = $state("idle");

  let glowColor = $derived(
    overlay?.type === "LowBattery" ? "rgba(255,59,92,0.45)"
    : overlay?.type === "AppToast" ? "rgba(10,132,255,0.32)"
    : mode === "chrome_focus"      ? "rgba(66,133,244,0.22)"
    : sysAccent                   ? `${sysAccent}55`
    : media.accent !== "#a0a0a0"   ? `${media.accent}55`
    : "rgba(255,255,255,0.04)"
  );

  function clamp01(x) { return Math.min(1, Math.max(0, x)); }
  function clampPct(p) { return Math.min(100, Math.max(0, Math.round(p))); }

  async function invokeSafe(cmd, args) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke(cmd, args);
    } catch {
      return null;
    }
  }

  async function refreshSystemSnapshot() {
    clearTimeout(_snapTimer);
    _snapTimer = setTimeout(async () => {
      const [vol, bri, accent, radios, dnd, theme] = await Promise.all([
        invokeSafe("get_volume"),
        invokeSafe("get_brightness"),
        invokeSafe("get_accent_color"),
        invokeSafe("get_radios_state"),
        invokeSafe("get_focus_assist"),
        invokeSafe("get_theme_mode"),
      ]);
      if (vol?.percent !== undefined) sysVolume = { percent: vol.percent, muted: !!vol.muted };
      if (bri) {
        if (bri.supported === false) sysBrightness = { supported: false, percent: sysBrightness.percent };
        else if (bri.percent !== undefined) sysBrightness = { supported: true, percent: bri.percent };
      }
      if (typeof accent === "string" && accent.startsWith("#")) sysAccent = accent;
      if (radios) {
        sysRadios = { wifi: radios.wifi ?? null, bluetooth: radios.bluetooth ?? null };
      }
      if (dnd) {
        if (dnd.supported === false) sysDnd = { supported: false, on: null };
        else sysDnd = { supported: true, on: dnd.on ?? null };
      }
      if (theme?.mode) sysTheme = { mode: theme.mode };

      const list = await invokeSafe("bookmark_list");
      if (Array.isArray(list)) bookmarksList = list;
      const u = (chromeBar?.url ?? "").trim();
      if (u) {
        const ex = await invokeSafe("bookmark_exists", { url: u });
        bookmarkPageSaved = !!ex;
      } else bookmarkPageSaved = false;
    }, 80);
  }

  async function addCurrentPageBookmark(e) {
    e?.stopPropagation();
    if (!canBookmarkFromChrome) return false;
    const url = bookmarkTargetUrl;
    const title = (chromeBar?.page_title ?? "").trim() || url;
    const r = await invokeSafe("bookmark_add", { url, title });
    if (r?.id != null) {
      bookmarkPageSaved = true;
      const list = await invokeSafe("bookmark_list");
      if (Array.isArray(list)) bookmarksList = list;
      return true;
    }
    return false;
  }

  async function openSavedBookmark(b, e) {
    e?.stopPropagation();
    await invokeSafe("bookmark_open_url", { url: b.url });
  }

  async function removeSavedBookmark(id, e) {
    e?.stopPropagation();
    await invokeSafe("bookmark_remove", { id });
    const list = await invokeSafe("bookmark_list");
    if (Array.isArray(list)) bookmarksList = list;
    const u = (chromeBar?.url ?? "").trim();
    if (u) {
      const ex = await invokeSafe("bookmark_exists", { url: u });
      bookmarkPageSaved = !!ex;
    } else bookmarkPageSaved = false;
  }

  async function openBookmarksPanelPage(e) {
    e?.stopPropagation();
    jelly();
    expandedPanelPage = "bookmarks";
    const list = await invokeSafe("bookmark_list");
    if (Array.isArray(list)) bookmarksList = list;
  }

  function closeBookmarksPanelPage(e) {
    e?.stopPropagation();
    expandedPanelPage = "main";
  }

  async function toggleSystemTheme(e) {
    e?.stopPropagation();
    const next = sysTheme.mode === "dark" ? "light" : "dark";
    sysTheme = { mode: next };
    const ok = await invokeSafe("set_theme_mode", { mode: next });
    if (!ok) {
      // 失败回退：打开系统设置
      await openSystem("ms-settings:colors", e);
    }
  }

  function percentFromPointer(e, el) {
    const r = el.getBoundingClientRect();
    const x = (e.clientX - r.left) / Math.max(1, r.width);
    return clampPct(clamp01(x) * 100);
  }

  function percentFromPointerY(e, el) {
    const r = el.getBoundingClientRect();
    const y = (e.clientY - r.top) / Math.max(1, r.height);
    return clampPct((1 - clamp01(y)) * 100);
  }

  async function setVolumePercent(p) {
    sysVolume = { ...sysVolume, percent: p };
    await invokeSafe("set_volume", { percent: p });
  }

  async function toggleMute(e) {
    e?.stopPropagation();
    const next = !sysVolume.muted;
    sysVolume = { ...sysVolume, muted: next };
    await invokeSafe("set_mute", { muted: next });
  }

  async function setBrightnessPercent(p) {
    sysBrightness = { ...sysBrightness, percent: p };
    await invokeSafe("set_brightness", { percent: p });
  }

  async function toggleRadio(kind, e) {
    e?.stopPropagation();
    const cur = kind === "wifi" ? sysRadios.wifi : sysRadios.bluetooth;
    if (cur === null) return;
    const next = !cur;
    sysRadios = { ...sysRadios, [kind]: next };
    await invokeSafe("set_radio_state", { kind, on: next });
  }

  async function toggleDnd(e) {
    e?.stopPropagation();
    if (!sysDnd.supported || sysDnd.on === null) return;
    const next = !sysDnd.on;
    sysDnd = { ...sysDnd, on: next };
    const ok = await invokeSafe("set_focus_assist", { on: next });
    if (ok === null) sysDnd = { ...sysDnd, on: null };
  }

  async function openSystem(uri, e) {
    e?.stopPropagation();
    await invokeSafe("open_system_uri", { uri });
  }

  async function doScreenshot(e) {
    e?.stopPropagation();
    await invokeSafe("screenshot");
  }

  async function openQuickSettings(e) {
    e?.stopPropagation();
    await invokeSafe("open_quick_settings");
  }

  async function toggleChromeMute(e) {
    e?.stopPropagation();
    if (!chromeBar?.has_sessions) return;
    const nextMuted = !chromeBar.chrome_muted;
    const ok = await invokeSafe("set_chrome_sessions_mute", { muted: nextMuted });
    if (ok !== true) return;
    chromeBar = {
      ...chromeBar,
      chrome_muted: nextMuted,
      audible: nextMuted ? false : chromeBar.audible,
    };
  }

  async function copyChromeText(text, e) {
    e?.stopPropagation();
    const t = String(text ?? "").trim();
    if (!t) return false;
    const ok = await invokeSafe("copy_text_to_clipboard", { text: t });
    if (ok === true) return true;
    try {
      await navigator.clipboard.writeText(t);
      return true;
    } catch {
      return false;
    }
  }

  function jelly() { jellyKey = (jellyKey + 1) % 1_000_000; }

  /**
   * 全部功能 ↔ 收藏：iOS 导航栈式整页切换（侧向推入 + 轻缩放，像新页面叠上来再重绘一档）
   * @param {Element} node
   * @param {{ page?: 'main' | 'bookmarks' }} params  本实例对应的子页（由 {#key} 内 @const 固定，避免 out 读到新状态）
   */
  function expandedIosOut(node, params) {
    const page = params?.page ?? "main";
    const duration = 320;
    const dx = page === "main" ? -40 : 48;
    return {
      duration,
      easing: cubicOut,
      css: (t) => {
        const u = 1 - t;
        const x = dx * u;
        const op = 0.38 + 0.62 * t;
        const s = 1 - 0.014 * u;
        return `transform: translate3d(${x}px,0,0) scale(${s}); opacity: ${op};`;
      },
    };
  }

  /** @param {Element} node @param {{ page?: 'main' | 'bookmarks' }} params */
  function expandedIosIn(node, params) {
    const page = params?.page ?? "main";
    const duration = 420;
    const dx = page === "bookmarks" ? 100 : -78;
    return {
      duration,
      easing: cubicOut,
      css: (t, u) => {
        const x = dx * u;
        const op = t;
        const s = 0.965 + 0.035 * t;
        return `transform: translate3d(${x}px,0,0) scale(${s}); opacity: ${op};`;
      },
    };
  }

  const BARS = [
    { color: "#ff3b5c", delay: "0ms",   minH: 20, maxH: 90  },
    { color: "#ff9f0a", delay: "110ms", minH: 40, maxH: 100 },
    { color: "#34c759", delay: "55ms",  minH: 55, maxH: 100 },
    { color: "#0a84ff", delay: "165ms", minH: 30, maxH: 85  },
    { color: "#bf5af2", delay: "85ms",  minH: 50, maxH: 95  },
  ];

  let batteryIcon = $derived(
    battery.charging ? "solar:battery-charge-bold"
    : battery.percent > 80 ? "solar:battery-full-bold"
    : battery.percent > 40 ? "solar:battery-bold"
    : "solar:battery-low-bold"
  );
  let batteryColor = $derived(
    battery.charging ? "#34c759"
    : battery.percent > 40 ? "#fff"
    : battery.percent > 15 ? "#ff9f0a"
    : "#ff3b5c"
  );

  let showSkipOverlay = $derived(hovered && !overlay && mode === "media");
  let lyricLine = $derived(firstLyricLine(lyrics));

  let chargeColor = $derived(
    battery.percent > 40 ? "#34c759"
    : battery.percent > 15 ? "#ff9f0a"
    : "#ff3b5c"
  );
  const CHARGE_CIRC = 100.5;
  let chargeDash = $derived((battery.percent / 100) * CHARGE_CIRC);

  let volPct   = $derived(overlay?.type === "VolumeChange" ? overlay.percent : 0);
  let volMuted = $derived(overlay?.type === "VolumeChange" ? overlay.muted   : false);
  let volIcon  = $derived(
    volMuted || volPct === 0 ? "solar:volume-cross-bold"
    : volPct < 40            ? "solar:volume-small-bold"
    : "solar:volume-loud-bold"
  );

</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->

<div class="island-scene">
  <div
    class="island"
    class:pressed
    style:width="{$w}px"
    style:height="{$h}px"
    style:border-radius="{$r}px"
    onmouseenter={onHoverEnter}
    onmouseleave={onHoverLeave}
    onclick={onClick}
  >
    <div class="blur-bg" style:border-radius="{$r}px" style:--glow={glowColor}></div>

    <!-- ══ 瞬时覆盖层 ══ -->
    {#if overlay}
      <div
        class="overlay-layer"
        class:overlay-toast-puff={overlay.type === "AppToast"}
        style:opacity={overlayOpacity}
      >

        {#if overlay.type === "VolumeChange"}
          <div class="ov-vol">
            <Icon icon={volIcon} width="18" color={volMuted ? "rgba(255,255,255,0.35)" : "#fff"} />
            <div class="vol-track">
              <div class="vol-fill" style:width="{volMuted ? 0 : volPct}%"></div>
            </div>
            <span class="vol-pct">{volMuted ? "静音" : volPct + "%"}</span>
          </div>

        {:else if overlay.type === "CapsLock"}
          <div class="ov-caps">
            <span class="caps-letter">{overlay.on ? "A" : "a"}</span>
            <span class="caps-label">{overlay.on ? "大写已开启" : "大写已关闭"}</span>
          </div>

        {:else if overlay.type === "LowBattery"}
          <div class="ov-lowbat">
            <Icon icon="solar:battery-low-bold" width="20" color="#ff3b5c" />
            <div class="lowbat-info">
              <span class="lowbat-pct">{overlay.percent}%</span>
              <span class="lowbat-label">电量不足</span>
            </div>
          </div>

        {:else if overlay.type === "WeChat"}
          <div class="ov-wechat">
            <Icon icon="simple-icons:wechat" width="20" color="#07c160" />
            <div class="wechat-line">
              <span class="wechat-app">微信</span>
              {#if overlay.kind === "call"}
                <span class="wechat-sep">·</span>
                <span class="wechat-tag">通话</span>
              {/if}
              <span class="wechat-sep">·</span>
              <span class="wechat-name" title={overlay.title}>{overlay.title}</span>
            </div>
          </div>

        {:else if overlay.type === "AppToast"}
          {@const l1 = overlay.title?.trim() ? overlay.title : overlay.app_name}
          {@const l2 = overlay.body?.trim()
            ? overlay.body
            : overlay.title?.trim()
              ? overlay.app_name
              : ""}
          {@const appIc = resolveAppToastIcon(overlay.aumid, overlay.app_name)}
          <div class="ov-app-toast">
            {#if overlay.icon_base64}
              <img
                class="app-toast-icon-img"
                src={overlay.icon_base64.startsWith("data:") ? overlay.icon_base64 : `data:image/png;base64,${overlay.icon_base64}`}
                alt=""
              />
            {:else}
              <div
                class="app-toast-icon-fallback"
                class:app-toast-icon-fallback--multicolor={appIc.icon.startsWith("vscode-icons:")}
                aria-hidden="true"
              >
                <Icon
                  icon={appIc.icon}
                  width={appIc.icon.startsWith("vscode-icons:") ? 30 : 26}
                  color={appIc.color === null ? undefined : appIc.color}
                />
              </div>
            {/if}
            <div class="app-toast-text">
              <div class="app-toast-line1" title={l1}>{l1}</div>
              {#if l2}
                <div class="app-toast-line2" title={l2}>{l2}</div>
              {/if}
            </div>
          </div>
        {/if}

      </div>
    {/if}

    <!-- ══ 主内容 ══ -->
    <div class="content" class:expanded={mode === "expanded"} style:opacity={overlay ? 0 : contentOpacity}>
      <div class="dbg">
        {#if dbg.err}<span class="err">{dbg.err}</span>{/if}
      </div>

      <!-- idle - 加载中动画 -->
      {#if mode === "idle"}
        <div class="loading-dots">
          <span></span><span></span><span></span>
        </div>

      <!-- charging - 简化单行显示 -->
      {:else if mode === "charging"}
        <div class="simple-charge">
          <Icon icon="solar:bolt-bold" width="16" color={chargeColor} />
          <span class="charge-text">{battery.percent}%</span>
        </div>

      <!-- Chrome 前台跟随 -->
      {:else if mode === "chrome_focus" && chromeBar}
        <ChromeFollowBar
          bar={chromeBar}
          onToggleMute={toggleChromeMute}
          onCopyText={copyChromeText}
          pageBookmarked={bookmarkPageSaved}
          canBookmark={canBookmarkFromChrome}
          onBookmark={addCurrentPageBookmark}
        />

      <!-- media -->
      {:else if mode === "media"}
        <div class="media-row">
          <div class="album-art">
            <MediaCover
              thumbnail={media.thumbnail}
              coverUrl={coverUrl}
              platformIcon={platformIcon(media.source)}
              size={34}
            />
          </div>
          <div class="track-info">
            <span class="track-name">{media.title}{media.artist ? ` - ${media.artist}` : ''}</span>
          </div>
          <div class="spectrum-zone">
            {#if showSkipOverlay}
              <!-- 悬浮时显示控制按钮 -->
              <div class="media-controls">
                <button class="ctrl-btn" onclick={(e) => ctrlMedia("prev", e)}>
                  <Icon icon="solar:skip-previous-bold" width="14" color="rgba(255,255,255,0.85)" />
                </button>
                <button class="ctrl-btn" onclick={(e) => ctrlMedia("play_pause", e)}>
                  <Icon icon={isPlayingUi ? "solar:pause-bold" : "solar:play-bold"} width="16" color="#fff" />
                </button>
                <button class="ctrl-btn" onclick={(e) => ctrlMedia("next", e)}>
                  <Icon icon="solar:skip-next-bold" width="14" color="rgba(255,255,255,0.85)" />
                </button>
              </div>
            {:else if isPlayingUi}
              <!-- 播放时显示动画（isPlayingUi 含乐观状态，避免 SMTC 暂停仍报 playing） -->
              <div class="spectrum">
                {#each BARS as bar}
                  <span class="bar"
                    style:background={bar.color}
                    style:animation-delay={bar.delay}
                    style:--min-h="{bar.minH}%"
                    style:--max-h="{bar.maxH}%"
                  ></span>
                {/each}
              </div>
            {:else}
              <!-- 暂停时显示暂停图标 -->
              <Icon icon="solar:pause-bold" width="16" color="rgba(255,255,255,0.3)" />
            {/if}
          </div>
        </div>

      <!-- expanded -->
      {:else if mode === "expanded"}
        <div class="expanded-wrap" class:expanded-wrap--bookmarks={expandedPanelPage === "bookmarks"}>
          {#key expandedPanelPage}
            {@const swapPage = expandedPanelPage}
            <div
              class="exp-swap-surface"
              class:exp-swap-surface--bookmarks={swapPage === "bookmarks"}
              in:expandedIosIn={{ page: swapPage }}
              out:expandedIosOut={{ page: swapPage }}
            >
              {#if swapPage === "main"}
                <div class="exp-media-strip">
                  <div class="exp-top">
                    <div class="exp-album">
                      <MediaCover
                        thumbnail={media.thumbnail}
                        coverUrl={coverUrl}
                        platformIcon={platformIcon(media.source)}
                        size={46}
                      />
                    </div>
                    <div class="exp-track">
                      <span class="exp-title">{media.title}</span>
                      <span class="exp-artist-lyric">{media.artist}{curLyricIndex >= 0 && lyricLines[curLyricIndex]?.text ? ` · ${lyricLines[curLyricIndex].text}` : ''}</span>
                    </div>
                    <div class="exp-controls">
                      <div class="exp-transport" role="group" aria-label="播放控制">
                        <button class="exp-ctrl-btn" type="button" title="上一首" aria-label="上一首" onclick={(e) => ctrlMedia("prev", e)}>
                          <Icon icon="solar:skip-previous-bold" width="20" color="rgba(255,255,255,0.88)" />
                        </button>
                        <button class="exp-play-btn" type="button" title="播放/暂停" aria-label="播放/暂停" onclick={(e) => ctrlMedia("play_pause", e)}>
                          <Icon icon={playIcon} width="26" color="#fff" />
                        </button>
                        <button class="exp-ctrl-btn" type="button" title="下一首" aria-label="下一首" onclick={(e) => ctrlMedia("next", e)}>
                          <Icon icon="solar:skip-next-bold" width="20" color="rgba(255,255,255,0.88)" />
                        </button>
                      </div>
                      <span class="exp-controls-sep" aria-hidden="true"></span>
                      <button
                        class="exp-open-btn"
                        type="button"
                        title="打开播放器"
                        aria-label="打开播放器"
                        disabled={!canOpenPlayer}
                        onclick={(e) => openPlayingApp(e)}
                      >
                        <Icon
                          icon="solar:square-arrow-right-up-bold"
                          width="18"
                          color={canOpenPlayer ? "rgba(255,255,255,0.9)" : "rgba(255,255,255,0.28)"}
                        />
                      </button>
                    </div>
                  </div>
                </div>
                <div class="panel">
                  <div class="panel-row">
                    <div class="left">
                      <div class="icon-grid big" style:--jkey={jellyKey}>
                        <button class="icon-tile big" title="深色/浅色" aria-label="深色/浅色" onclick={(e) => { e.stopPropagation(); jelly(); toggleSystemTheme(e); }}>
                          <Icon icon={sysTheme.mode === "dark" ? "solar:moon-bold" : "solar:sun-bold"} width="18" />
                        </button>
                        <button class="icon-tile big" title="电源与电池" aria-label="电源与电池" onclick={(e) => { e.stopPropagation(); jelly(); openSystem("ms-settings:batterysaver", e); }}>
                          <Icon icon={batteryIcon} width="18" color={batteryColor} />
                        </button>
                        <button class="icon-tile big" title="Wi‑Fi 设置" aria-label="Wi‑Fi 设置" onclick={(e) => { e.stopPropagation(); jelly(); openSystem("ms-settings:network-wifi", e); }}>
                          <Icon icon="material-symbols:wifi-rounded" width="18" />
                        </button>
                        <button class="icon-tile big" title="蓝牙设置" aria-label="蓝牙设置" onclick={(e) => { e.stopPropagation(); jelly(); openSystem("ms-settings:bluetooth", e); }}>
                          <Icon icon="solar:bluetooth-bold" width="18" />
                        </button>
                        <button class="icon-tile big" title="免打扰设置" aria-label="免打扰设置" onclick={(e) => { e.stopPropagation(); jelly(); openSystem("ms-settings:quietmomentshome", e); }}>
                          <Icon icon="solar:moon-bold" width="18" />
                        </button>
                        <button class="icon-tile big" title="截图" aria-label="截图" onclick={(e) => { e.stopPropagation(); jelly(); doScreenshot(e); }}>
                          <Icon icon="solar:camera-bold" width="18" />
                        </button>
                        <button class="icon-tile big" title="快速设置" aria-label="快速设置" onclick={(e) => { e.stopPropagation(); jelly(); openQuickSettings(e); }}>
                          <Icon icon="solar:widget-2-bold" width="18" />
                        </button>
                        <button
                          class="icon-tile big"
                          title="收藏（Chrome 条星标可添加当前页）"
                          aria-label="打开收藏列表"
                          onclick={(e) => { e.stopPropagation(); openBookmarksPanelPage(e); }}
                        >
                          <Icon icon="solar:bookmark-bold" width="18" color="rgba(255,255,255,0.88)" />
                        </button>
                      </div>
                    </div>

                    <div class="right">
                      <div class="cc">
                        <div
                          class="v-slider"
                          title="亮度"
                          aria-label="亮度"
                          class:disabled={!sysBrightness.supported}
                          onpointerdown={(e) => {
                            if (!sysBrightness.supported) return;
                            e.stopPropagation();
                            const el = e.currentTarget;
                            el.setPointerCapture(e.pointerId);
                            _drag = "bri";
                            setBrightnessPercent(percentFromPointerY(e, el));
                          }}
                          onpointermove={(e) => {
                            if (_drag !== "bri") return;
                            const el = e.currentTarget;
                            setBrightnessPercent(percentFromPointerY(e, el));
                          }}
                          onpointerup={(e) => { _drag = null; try { e.currentTarget.releasePointerCapture(e.pointerId); } catch {} }}
                          onpointercancel={() => { _drag = null; }}
                        >
                          <div class="v-fill bright" style:height="{sysBrightness.percent}%"></div>
                          <div class="v-ico" title="亮度" aria-label="亮度">
                            <Icon icon="solar:sun-bold" width="16" />
                          </div>
                          <div class="v-val">{sysBrightness.supported ? sysBrightness.percent + "%" : "不支持"}</div>
                        </div>

                        <div
                          class="v-slider"
                          title="音量"
                          aria-label="音量"
                          onpointerdown={(e) => {
                            e.stopPropagation();
                            const el = e.currentTarget;
                            el.setPointerCapture(e.pointerId);
                            _drag = "vol";
                            setVolumePercent(percentFromPointerY(e, el));
                          }}
                          onpointermove={(e) => {
                            if (_drag !== "vol") return;
                            const el = e.currentTarget;
                            setVolumePercent(percentFromPointerY(e, el));
                          }}
                          onpointerup={(e) => { _drag = null; try { e.currentTarget.releasePointerCapture(e.pointerId); } catch {} }}
                          onpointercancel={() => { _drag = null; }}
                        >
                          <div class="v-fill" style:height="{sysVolume.muted ? 0 : sysVolume.percent}%"></div>
                          <button class="v-ico" onclick={toggleMute} title="静音" aria-label="静音">
                            <Icon icon={sysVolume.muted || sysVolume.percent === 0 ? "solar:volume-cross-bold" : "solar:volume-loud-bold"} width="16" />
                          </button>
                          <div class="v-val">{sysVolume.muted ? "静音" : sysVolume.percent + "%"}</div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              {:else}
                <div class="panel panel-bookmarks">
                  <div class="bm-view-head">
                    <button
                      type="button"
                      class="bm-back"
                      title="返回"
                      aria-label="返回全部功能"
                      onclick={(e) => { e.stopPropagation(); jelly(); closeBookmarksPanelPage(e); }}
                    >
                      <Icon icon="solar:alt-arrow-left-linear" width="22" color="rgba(255,255,255,0.9)" />
                    </button>
                    <span class="bm-view-title">收藏</span>
                    <button
                      type="button"
                      class="bm-head-star"
                      title={canBookmarkFromChrome ? (bookmarkPageSaved ? "已收藏（再点更新标题）" : "添加当前 Chrome 页") : "请切到 Chrome 且能读到地址栏"}
                      aria-label="收藏当前页"
                      disabled={!canBookmarkFromChrome}
                      onclick={(e) => { e.stopPropagation(); jelly(); addCurrentPageBookmark(e); }}
                    >
                      <Icon
                        icon={bookmarkPageSaved ? "solar:star-bold" : "solar:star-outline"}
                        width="18"
                        color={canBookmarkFromChrome ? (bookmarkPageSaved ? "#ffcc00" : "rgba(255,255,255,0.88)") : "rgba(255,255,255,0.28)"}
                      />
                    </button>
                  </div>
                  <div class="bm-list-outer">
                    <ul class="bm-list bm-list--full">
                      {#each bookmarksList as b (b.id)}
                        <li class="bm-item">
                          <button type="button" class="bm-open" onclick={(e) => openSavedBookmark(b, e)} title={b.url}>
                            <span class="bm-it">{b.title}</span>
                            <span class="bm-iu">{b.url}</span>
                          </button>
                          <button
                            type="button"
                            class="bm-del"
                            title="删除"
                            aria-label="删除收藏"
                            onclick={(e) => removeSavedBookmark(b.id, e)}
                          >
                            <Icon icon="solar:trash-bin-minimalistic-bold" width="15" color="rgba(255,255,255,0.45)" />
                          </button>
                        </li>
                      {:else}
                        <li class="bm-empty">暂无收藏。在 Chrome 前台时，点岛条上的星标即可添加。</li>
                      {/each}
                    </ul>
                  </div>
                </div>
              {/if}
            </div>
          {/key}
        </div>
      {/if}

    </div>
  </div>
</div>

<style>
  .island-scene { position: relative; display: flex; align-items: center; justify-content: center; }

  /* ══ 主岛 ══ */
  .island {
    position: relative;
    cursor: pointer;
    transition: transform 0.12s ease;
    overflow: hidden;
    will-change: width, height, border-radius;
    flex-shrink: 0;
  }
  .island.pressed { transform: scale(0.94); }

  .blur-bg {
    position: absolute; inset: 0;
    background: rgba(10,10,10,0.88);
    backdrop-filter: blur(28px) saturate(1.8);
    -webkit-backdrop-filter: blur(28px) saturate(1.8);
    border: 1px solid rgba(255,255,255,0.07);
    box-shadow:
      0 0 0 0.5px rgba(255,255,255,0.04) inset,
      0 8px 32px rgba(0,0,0,0.55),
      0 0 20px 3px var(--glow, rgba(255,255,255,0.04));
    transition: box-shadow 0.6s ease;
  }

  .content {
    position: relative; z-index: 1;
    width: 100%; height: 100%;
    display: flex; align-items: center; justify-content: center;
    padding: 0 12px;
    transition: opacity 0.16s ease;
    color: #fff;
  }
  .content.expanded {
    align-items: flex-start;
    justify-content: flex-start;
    padding-top: 12px;
    width: 100%;
    box-sizing: border-box;
  }

  .dbg{
    position: absolute;
    left: 8px;
    bottom: 6px;
    display: flex;
    gap: 6px;
    font-size: 9px;
    color: rgba(255,255,255,0.45);
    pointer-events: none;
    max-width: calc(100% - 16px);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dbg .err{ color: rgba(255,59,92,0.9); }

  /* ══ overlay ══ */
  .overlay-layer {
    position: absolute; inset: 0; z-index: 10;
    display: flex; align-items: center; justify-content: center;
    padding: 0 14px;
    transition: opacity 0.18s ease;
    pointer-events: none;
  }

  .overlay-layer.overlay-toast-puff {
    transition: opacity 0.22s ease-out;
  }
  .overlay-layer.overlay-toast-puff .ov-app-toast {
    animation: island-toast-pop 0.42s cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  @keyframes island-toast-pop {
    0% { transform: scale(0.94) translateY(8px); opacity: 0.6; }
    100% { transform: scale(1) translateY(0); opacity: 1; }
  }

  .ov-app-toast {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-width: 0;
    padding: 2px 0;
  }
  .app-toast-icon-img {
    width: 38px;
    height: 38px;
    border-radius: 10px;
    object-fit: cover;
    flex-shrink: 0;
  }
  /* 无系统 Logo 时：圆角「应用标」占位，不用灰方块 + 窗口线框 */
  .app-toast-icon-fallback {
    width: 40px;
    height: 40px;
    border-radius: 11px;
    background: linear-gradient(155deg, rgba(255,255,255,0.14), rgba(255,255,255,0.04));
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.12), 0 2px 12px rgba(0,0,0,0.28);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .app-toast-icon-fallback--multicolor {
    background: rgba(255,255,255,0.06);
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.08), 0 2px 10px rgba(0,0,0,0.2);
  }
  .app-toast-text {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 5px;
    justify-content: center;
  }
  .app-toast-line1 {
    font-size: 13px;
    font-weight: 700;
    color: #fff;
    line-height: 1.28;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .app-toast-line2 {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255,255,255,0.55);
    line-height: 1.3;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ov-vol { display: flex; align-items: center; gap: 8px; width: 100%; }
  .vol-track {
    flex: 1; height: 4px; border-radius: 2px;
    background: rgba(255,255,255,0.15); overflow: hidden;
  }
  .vol-fill {
    height: 100%; border-radius: 2px;
    background: linear-gradient(90deg, #0a84ff, #34c759);
    transition: width 0.25s cubic-bezier(0.4,0,0.2,1);
  }
  .vol-pct { font-size: 10px; font-weight: 700; color: rgba(255,255,255,0.7); min-width: 28px; text-align: right; }

  .ov-caps { display: flex; align-items: center; gap: 8px; }
  .caps-letter { font-size: 22px; font-weight: 800; color: #fff; line-height: 1; }
  .caps-label { font-size: 11px; color: rgba(255,255,255,0.55); font-weight: 500; }

  .ov-lowbat { display: flex; align-items: center; gap: 8px; animation: lowbat-shake 0.4s ease 0.1s 2; }
  .lowbat-info { display: flex; flex-direction: column; gap: 1px; }
  .lowbat-pct { font-size: 15px; font-weight: 800; color: #ff3b5c; line-height: 1; }
  .lowbat-label { font-size: 9px; color: rgba(255,59,92,0.7); font-weight: 600; }
  @keyframes lowbat-shake {
    0%,100% { transform: translateX(0); }
    25%     { transform: translateX(-3px); }
    75%     { transform: translateX(3px); }
  }

  .ov-wechat {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-width: 0;
  }
  .wechat-line {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    flex: 1;
    font-size: 12px;
    line-height: 1.2;
  }
  .wechat-app {
    flex-shrink: 0;
    font-weight: 700;
    color: rgba(7, 193, 96, 0.95);
  }
  .wechat-sep {
    flex-shrink: 0;
    color: rgba(255,255,255,0.32);
    font-weight: 500;
  }
  .wechat-tag {
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 700;
    color: rgba(255, 159, 10, 0.95);
  }
  .wechat-name {
    min-width: 0;
    flex: 1;
    font-weight: 600;
    color: #fff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* idle */
  /* 删除旧的 dot 样式 */

  /* charging - 简化单行 */
  .simple-charge { 
    display: flex; 
    align-items: center; 
    gap: 6px; 
    justify-content: center;
    padding: 0 16px;
  }
  .charge-text { 
    font-size: 13px; 
    font-weight: 600; 
    color: rgba(255,255,255,0.8);
  }
  
  /* 加载中动画 */
  .loading-dots {
    display: flex;
    gap: 4px;
    align-items: center;
    justify-content: center;
    padding: 0 20px;
  }
  .loading-dots span {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: rgba(255,255,255,0.5);
    animation: loading-bounce 1.4s ease-in-out infinite both;
  }
  .loading-dots span:nth-child(1) { animation-delay: -0.32s; }
  .loading-dots span:nth-child(2) { animation-delay: -0.16s; }
  @keyframes loading-bounce {
    0%, 80%, 100% { transform: scale(0.6); opacity: 0.5; }
    40% { transform: scale(1); opacity: 1; }
  }

  /* media / dual */
  .media-row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 0 10px; }
  .album-art {
    width: 28px; height: 28px; border-radius: 6px;
    background: rgba(255,255,255,0.08);
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0; overflow: hidden; position: relative;
  }
  .track-info { flex: 1; display: flex; flex-direction: column; gap: 2px; overflow: hidden; min-width: 0; }
  .track-name { font-size: 12px; font-weight: 650; color: #fff; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .track-artist { font-size: 10px; color: rgba(255,255,255,0.60); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  /* 滚动歌词样式 */
  .lyrics-scroll-container {
    height: 18px;
    overflow: hidden;
    position: relative;
  }
  .lyrics-list {
    display: flex;
    flex-direction: column;
    transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .lyric-line {
    font-size: 10px;
    line-height: 18px;
    height: 18px;
    color: rgba(255,255,255,0.40);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: all 0.3s ease;
  }
  .lyric-line.active {
    color: rgba(255,255,255,0.90);
    font-weight: 600;
    text-shadow: 0 0 8px rgba(255,255,255,0.3);
  }

  .spectrum-zone { flex-shrink: 0; width: 56px; height: 34px; display: flex; align-items: center; justify-content: center; }
  .spectrum { display: flex; align-items: flex-end; gap: 3px; height: 22px; }
  .bar { display: block; width: 3px; border-radius: 2px; height: var(--min-h); animation: bar-bounce 0.7s ease-in-out infinite alternate; }
  @keyframes bar-bounce { from { height: var(--min-h); } to { height: var(--max-h); } }

  .media-controls { display: flex; align-items: center; gap: 2px; }
  .ctrl-btn {
    background: transparent; border: none; cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    padding: 3px; border-radius: 50%; transition: background 0.12s;
  }
  .ctrl-btn:hover { background: rgba(255,255,255,0.12); }

  /* expanded */
  .expanded-wrap {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
    min-height: 0;
    padding: 6px 0 4px 0;
    overflow: hidden;
    border-radius: inherit;
  }
  .expanded-wrap--bookmarks {
    flex: 1;
    min-height: 0;
    padding-top: 4px;
  }
  .exp-swap-surface {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
    min-height: 0;
    transform-origin: 50% 45%;
    will-change: transform, opacity;
    backface-visibility: hidden;
  }
  .exp-swap-surface--bookmarks {
    flex: 1;
    min-height: 0;
  }
  .exp-media-strip {
    width: 100%;
    padding: 6px 2px 8px;
    border-radius: 0;
    background: transparent;
    border: none;
    box-shadow: none;
  }
  .exp-top { display: flex; align-items: center; gap: 12px; width: 100%; }
  .exp-album {
    width: 46px; height: 46px; border-radius: 11px;
    background: rgba(0,0,0,0.25);
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0; overflow: hidden;
    box-shadow: 0 0 0 1px rgba(255,255,255,0.06) inset;
  }
  .exp-track { flex: 1; display: flex; flex-direction: column; gap: 3px; overflow: hidden; min-width: 0; }
  .exp-title { font-size: 13px; font-weight: 600; color: #fff; letter-spacing: 0.01em; line-height: 1.25; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* 展开模式歌手+歌词 - 一行显示 */
  .exp-artist-lyric {
    font-size: 10px;
    line-height: 1.3;
    color: rgba(255,255,255,0.48);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .exp-controls {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .exp-transport {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0;
    border-radius: 0;
    background: transparent;
    border: none;
    box-shadow: none;
  }
  .exp-controls-sep {
    display: none;
  }
  .exp-ctrl-btn {
    flex-shrink: 0;
    background: transparent;
    border: none;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: 50%;
    transition: background 0.14s ease, transform 0.12s ease;
  }
  .exp-ctrl-btn:hover:not(:disabled) { background: rgba(255,255,255,0.12); }
  .exp-ctrl-btn:active:not(:disabled) { transform: scale(0.94); }

  .exp-play-btn {
    flex-shrink: 0;
    width: 38px;
    height: 38px;
    background: transparent;
    border: none;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    margin: 0;
    border-radius: 50%;
    transition: background 0.14s ease, transform 0.12s ease;
    box-shadow: none;
  }
  .exp-play-btn:hover { background: rgba(255,255,255,0.1); }
  .exp-play-btn:active { transform: scale(0.95); }

  .exp-open-btn {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 50%;
    cursor: pointer;
    background: transparent;
    border: none;
    transition: background 0.14s ease, transform 0.12s ease;
  }
  .exp-open-btn:hover:not(:disabled) { background: rgba(255,255,255,0.1); }
  .exp-open-btn:active:not(:disabled) { transform: scale(0.96); }
  .exp-open-btn:disabled { cursor: default; opacity: 0.4; }
  .panel {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 10px;
    /* 不滚动：内容一次性展示 */
  }

  .panel-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 12px;
    align-items: start;
  }
  .left { min-width: 0; }
  .right { width: 156px; }

  .bm-list {
    list-style: none;
    margin: 0;
    padding: 0 2px 0 0;
    max-height: 96px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  /**
   * 收藏列表：高度随内容增长，最高 220px（与原展开区列表上限一致），超出滚动。
   */
  .bm-list-outer {
    width: 100%;
    max-height: 220px;
    overflow-x: hidden;
    overflow-y: auto;
    min-height: 0;
    border-radius: 12px;
    box-sizing: border-box;
  }
  .bm-list--full {
    max-height: none;
    overflow: visible;
    width: 100%;
  }
  .panel-bookmarks {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    align-self: flex-start;
  }
  .bm-view-head {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding: 0 0 6px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }
  .bm-back {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    border: none;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.08);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.12s ease;
  }
  .bm-back:hover {
    background: rgba(255, 255, 255, 0.14);
  }
  .bm-view-title {
    flex: 1;
    font-size: 14px;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.95);
    text-align: center;
  }
  .bm-head-star {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    border: none;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.08);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.12s ease;
  }
  .bm-head-star:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.14);
  }
  .bm-head-star:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .bm-item {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    gap: 4px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.05);
    overflow: hidden;
  }
  .bm-open {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 7px 8px;
    border: none;
    background: transparent;
    color: #fff;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s ease;
  }
  .bm-open:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .bm-it {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    width: 100%;
  }
  .bm-iu {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.45);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    width: 100%;
  }
  .bm-del {
    flex-shrink: 0;
    width: 34px;
    border: none;
    background: transparent;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.12s ease;
  }
  .bm-del:hover {
    background: rgba(255, 59, 92, 0.15);
  }
  .bm-empty {
    padding: 10px 8px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.42);
    line-height: 1.35;
    list-style: none;
  }

  /* 图标面板（无背板框） */
  .icon-grid {
    display: grid;
    grid-template-columns: repeat(9, 1fr);
    gap: 8px;
    padding: 4px 0;
    border-radius: 0;
    background: transparent;
    border: none;
    box-shadow: none;
  }
  .icon-grid.big { grid-template-columns: repeat(3, 1fr); padding: 4px 0; }
  .left .icon-grid.big { padding: 6px 0; gap: 10px; }
  .icon-tile {
    height: 34px;
    border-radius: 12px;
    border: none;
    background: transparent;
    color: rgba(255,255,255,0.92);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: transform 0.12s ease, background 0.12s ease;
    animation: jelly 420ms cubic-bezier(0.2, 0.8, 0.2, 1);
    animation-play-state: paused;
  }
  .icon-tile:active { transform: scale(0.92); }
  .icon-tile:hover { background: rgba(255,255,255,0.08); }
  .icon-tile:disabled { opacity: 0.35; cursor: default; }
  .icon-tile.big {
    width: 50px;
    height: 50px;
    border-radius: 999px;
    justify-self: center;
    background: transparent;
    border: none;
    box-shadow: none;
  }
  .icon-tile.big:hover { background: rgba(255,255,255,0.08); }
  .icon-tile.big:active { transform: scale(0.90); }

  /* active state removed (no page switching) */

  .icon-grid { --_j: var(--jkey); }
  .icon-grid[style] .icon-tile { animation-play-state: running; animation-name: jelly; animation-delay: calc(var(--_j) * 0s); }
  @keyframes jelly {
    0%   { transform: scale(1); }
    22%  { transform: scaleX(1.18) scaleY(0.86); }
    48%  { transform: scaleX(0.92) scaleY(1.12); }
    72%  { transform: scaleX(1.06) scaleY(0.96); }
    100% { transform: scale(1); }
  }

  /* swatch styles removed (theme toggle uses sun/moon) */
  /* 控制中心：竖滑杆 */
  .cc {
    width: 100%;
    display: grid;
    grid-template-columns: 68px 68px;
    gap: 8px;
    padding: 10px 8px;
    border-radius: 18px;
    background: rgba(255,255,255,0.04);
    border: 1px solid rgba(255,255,255,0.05);
    justify-content: center;
  }
  .v-slider {
    position: relative;
    height: 132px;
    width: 68px;
    border-radius: 26px;
    background: rgba(255,255,255,0.035);
    border: 1px solid rgba(255,255,255,0.06);
    overflow: hidden;
    touch-action: none;
  }
  .v-slider.disabled { opacity: 0.35; }
  .v-fill {
    position: absolute;
    left: 10px; right: 10px; bottom: 10px;
    height: 40%;
    border-radius: 18px;
    background: rgba(10,132,255,0.9);
    filter: drop-shadow(0 8px 18px rgba(10,132,255,0.22));
  }
  .v-fill.bright {
    background: rgba(255,255,255,0.85);
    filter: drop-shadow(0 8px 18px rgba(255,255,255,0.12));
  }
  .v-ico {
    position: absolute;
    left: 50%;
    bottom: 10px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 999px;
    border: 1px solid rgba(255,255,255,0.10);
    background: rgba(0,0,0,0.10);
    color: rgba(255,255,255,0.9);
    cursor: pointer;
  }
  .v-val {
    position: absolute;
    top: 9px;
    left: 0;
    right: 0;
    text-align: center;
    font-size: 10px;
    font-weight: 900;
    color: rgba(255,255,255,0.7);
    letter-spacing: 0.2px;
    pointer-events: none;
  }

  /* horizontal slider styles removed (migrated to vertical sliders) */

  /* legacy swatch removed */

  /* legacy styles removed (migrated to icon-grid) */
</style>
