<script>
  import { onDestroy } from "svelte";
  import Icon from "@iconify/svelte";

  /**
   * @typedef {{ app_id: string, display_name: string, window_title: string, context_line?: string | null, needs_attention: boolean, minimized: boolean }} IdeBar
   * @type {{ bar: IdeBar, onCopyText: (text: string, e: Event) => boolean | Promise<boolean>, onFocusWindow: (e: Event) => void | Promise<void> }}
   */
  let { bar, onCopyText, onFocusWindow } = $props();

  const iconId = $derived(
    bar.app_id === "vscode"
      ? "simple-icons:visualstudiocode"
      : "vscode-icons:file-type-cursor"
  );

  const copySource = $derived.by(() => {
    const t = String(bar.window_title ?? "").trim();
    const c = String(bar.context_line ?? "").trim();
    if (c) return `${t}\n${c}`;
    return t;
  });

  let copyFlash = $state(false);
  let _copyTimer = null;

  onDestroy(() => {
    if (_copyTimer) clearTimeout(_copyTimer);
  });

  async function doCopy(e) {
    const text = copySource();
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
</script>

<div class="ide-row">
  <div class="ide-main">
    <div class="ide-icon" aria-hidden="true">
      <Icon icon={iconId} width="26" color="rgba(255,255,255,0.92)" />
    </div>
    <div class="ide-text-col">
      <span class="ide-label">
        {bar.display_name}
        {#if bar.minimized}
          <span class="ide-badge">已最小化</span>
        {/if}
        {#if bar.needs_attention}
          <span class="ide-badge ide-badge--warn">待确认</span>
        {/if}
      </span>
      {#if bar.context_line}
        <span class="ide-context" title={bar.context_line}>{bar.context_line}</span>
      {/if}
      <span class="ide-title" title={bar.window_title}>{bar.window_title}</span>
    </div>
  </div>

  <div class="ide-actions">
    <button
      type="button"
      class="ide-act-btn"
      class:ide-act-btn--ok={copyFlash}
      title={copyFlash ? "已复制" : "复制标题与摘要"}
      aria-label="复制窗口标题与可见摘要"
      onclick={(e) => doCopy(e)}
    >
      {#if copyFlash}
        <Icon icon="solar:check-circle-bold" width="18" color="#34c759" />
      {:else}
        <Icon icon="solar:copy-bold" width="17" color="rgba(255,255,255,0.88)" />
      {/if}
    </button>
    <button
      type="button"
      class="ide-act-btn"
      title="切回 {bar.display_name} 窗口"
      aria-label="将 IDE 窗口恢复到前台"
      onclick={(e) => onFocusWindow(e)}
    >
      <Icon icon="solar:window-frame-bold" width="18" color="rgba(255,255,255,0.88)" />
    </button>
  </div>
</div>

<style>
  .ide-row {
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
  .ide-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  .ide-icon {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
  }
  .ide-text-col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    line-height: 1.25;
  }
  .ide-label {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.42);
    line-height: 1.2;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .ide-badge {
    font-size: 8px;
    font-weight: 700;
    text-transform: none;
    letter-spacing: 0.02em;
    padding: 2px 6px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.75);
  }
  .ide-badge--warn {
    background: rgba(255, 159, 10, 0.22);
    color: #ffcc80;
  }
  .ide-context {
    font-size: 11px;
    font-weight: 500;
    color: rgba(200, 190, 255, 0.92);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ide-title {
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ide-actions {
    flex-shrink: 0;
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }
  .ide-act-btn {
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
  .ide-act-btn:hover {
    background: rgba(255, 255, 255, 0.16);
  }
  .ide-act-btn:active {
    transform: scale(0.94);
  }
  .ide-act-btn--ok {
    background: rgba(52, 199, 89, 0.22);
    box-shadow: 0 0 0 1px rgba(52, 199, 89, 0.35);
  }
  .ide-act-btn--ok:hover {
    background: rgba(52, 199, 89, 0.28);
  }
</style>
