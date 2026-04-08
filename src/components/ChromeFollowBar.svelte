<script>
  import { onDestroy } from "svelte";
  import Icon from "@iconify/svelte";

  /**
   * @typedef {{ page_title: string, url?: string | null, has_sessions: boolean, audible: boolean, chrome_muted: boolean }} ChromeBar
   * @type {{
   *   bar: ChromeBar,
   *   onToggleMute: (e: Event) => void,
   *   onCopyText: (text: string, e: Event) => boolean | Promise<boolean>,
   *   pageBookmarked?: boolean,
   *   canBookmark?: boolean,
   *   onBookmark?: (e: Event) => boolean | Promise<boolean>,
   * }}
   */
  let {
    bar,
    onToggleMute,
    onCopyText,
    pageBookmarked = false,
    canBookmark = false,
    onBookmark,
  } = $props();

  const showBookmark = $derived(typeof onBookmark === "function");

  const showMute = $derived(bar.has_sessions);
  const urlStr = $derived((bar.url ?? "").trim());
  const hasUrl = $derived(urlStr.length > 0);
  /** 复制成功短暂高亮 */
  let copyFlash = $state(false);
  let _copyTimer = null;
  /** 收藏成功短暂高亮 */
  let bookmarkFlash = $state(false);
  let _bmTimer = null;

  onDestroy(() => {
    if (_copyTimer) clearTimeout(_copyTimer);
    if (_bmTimer) clearTimeout(_bmTimer);
  });

  async function doCopy(e) {
    const text = hasUrl ? urlStr : String(bar.page_title ?? "").trim();
    if (!text) return;
    const ok = await onCopyText(text, e);
    if (!ok) return;
    copyFlash = true;
    if (_copyTimer) clearTimeout(_copyTimer);
    _copyTimer = setTimeout(() => {
      copyFlash = false;
      _copyTimer = null;
    }, 1400);
  }

  async function doBookmark(e) {
    if (!showBookmark || !canBookmark) return;
    const ok = await onBookmark(e);
    if (!ok) return;
    bookmarkFlash = true;
    if (_bmTimer) clearTimeout(_bmTimer);
    _bmTimer = setTimeout(() => {
      bookmarkFlash = false;
      _bmTimer = null;
    }, 1400);
  }
</script>

<div class="chrome-row">
  <div class="chrome-main">
    <div class="chrome-icon" aria-hidden="true">
      <Icon icon="logos:chrome" width="26" />
    </div>
    <div class="chrome-text-col">
      <span class="chrome-label">Chrome</span>
      {#if hasUrl}
        <span class="chrome-url" title={urlStr}>{urlStr}</span>
      {/if}
      <span class="chrome-page" title={bar.page_title}>{bar.page_title}</span>
    </div>
  </div>

  <div class="chrome-actions">
    <button
      type="button"
      class="chrome-act-btn"
      class:chrome-act-btn--ok={copyFlash}
      title={copyFlash ? "已复制" : hasUrl ? "复制链接" : "复制标题"}
      aria-label={copyFlash ? "已复制到剪贴板" : hasUrl ? "复制当前页链接" : "复制页面标题"}
      onclick={(e) => doCopy(e)}
    >
      {#if copyFlash}
        <Icon icon="solar:check-circle-bold" width="18" color="#34c759" />
      {:else}
        <Icon icon="solar:copy-bold" width="17" color="rgba(255,255,255,0.88)" />
      {/if}
    </button>
    {#if showBookmark}
      <button
        type="button"
        class="chrome-act-btn"
        class:chrome-act-btn--ok={bookmarkFlash}
        title={bookmarkFlash ? "已收藏" : pageBookmarked ? "已收藏（再点可更新标题）" : "收藏当前页"}
        aria-label={bookmarkFlash ? "已加入收藏" : pageBookmarked ? "更新收藏标题" : "收藏当前 Chrome 页面"}
        disabled={!canBookmark}
        onclick={(e) => doBookmark(e)}
      >
        {#if bookmarkFlash}
          <Icon icon="solar:check-circle-bold" width="18" color="#34c759" />
        {:else}
          <Icon
            icon={pageBookmarked ? "solar:star-bold" : "solar:star-outline"}
            width="18"
            color={pageBookmarked ? "#ffcc00" : "rgba(255,255,255,0.88)"}
          />
        {/if}
      </button>
    {/if}
    {#if showMute}
      <button
        type="button"
        class="chrome-act-btn"
        title={bar.chrome_muted ? "取消静音" : "静音网页声音"}
        aria-label={bar.chrome_muted ? "取消 Chrome 标签音频静音" : "静音 Chrome 标签音频"}
        onclick={(e) => onToggleMute(e)}
      >
        {#if bar.chrome_muted}
          <Icon icon="solar:volume-cross-bold" width="18" color="rgba(255,255,255,0.45)" />
        {:else if bar.audible}
          <Icon icon="solar:volume-loud-bold" width="18" color="#fff" />
        {:else}
          <Icon icon="solar:volume-small-bold" width="18" color="rgba(255,255,255,0.55)" />
        {/if}
      </button>
    {/if}
  </div>
</div>

<style>
  .chrome-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    min-width: 0;
    padding: 5px 4px 5px 2px;
    box-sizing: border-box;
  }
  .chrome-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  .chrome-icon {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
  }
  .chrome-text-col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    line-height: 1.25;
  }
  .chrome-label {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.42);
    line-height: 1.1;
  }
  .chrome-url {
    font-size: 11px;
    font-weight: 500;
    color: rgba(120, 170, 255, 0.96);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chrome-page {
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.92);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chrome-actions {
    flex-shrink: 0;
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }
  .chrome-act-btn {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.09);
    cursor: pointer;
    transition:
      background 0.15s ease,
      transform 0.12s ease,
      box-shadow 0.2s ease;
  }
  .chrome-act-btn:hover {
    background: rgba(255, 255, 255, 0.16);
  }
  .chrome-act-btn:active {
    transform: scale(0.94);
  }
  .chrome-act-btn--ok {
    background: rgba(52, 199, 89, 0.22);
    box-shadow: 0 0 0 1px rgba(52, 199, 89, 0.35);
  }
  .chrome-act-btn--ok:hover {
    background: rgba(52, 199, 89, 0.28);
  }
</style>
