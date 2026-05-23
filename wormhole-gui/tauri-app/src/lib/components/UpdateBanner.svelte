<script>
  import { updateState, updateDismissedThisSession } from '../store.js';
  import { applyUpdate } from '../ipc.js';

  // Format bytes as "1.4 MB" / "120 KB".
  function fmt(bytes) {
    if (bytes == null) return '';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  $: state = $updateState;
  $: pct =
    state && state.status === 'downloading' && state.total
      ? Math.min(100, Math.round((state.downloaded / state.total) * 100))
      : null;

  async function onApply() {
    // Swallow the version so we can show it during the download phase.
    updateState.set({
      status: 'downloading',
      version: state.version,
      downloaded: 0,
      total: null,
    });
    try {
      await applyUpdate();
      // If we ever return here, the backend didn't exit — treat it as an
      // error the user should see, so they can retry rather than wonder
      // why the app didn't restart.
      updateState.set({
        status: 'error',
        version: state.version,
        message: '更新已下载，但应用未自动重启',
      });
    } catch (err) {
      updateState.set({
        status: 'error',
        version: state.version,
        message: `${err}`,
      });
    }
  }

  function onDismiss() {
    updateDismissedThisSession.set(true);
    updateState.set(null);
  }

  function onCloseError() {
    updateState.set(null);
  }
</script>

{#if state}
  <div class="update-banner" class:err={state.status === 'error'}>
    {#if state.status === 'available'}
      <div class="row">
        <span class="msg">
          🔄 发现新版本 v{state.version}
          {#if state.form === 'portable'}
            <span class="form-hint">便携版会替换当前 exe</span>
          {/if}
        </span>
        <div class="actions">
          <button class="btn primary" on:click={onApply}>立即更新</button>
          <button class="btn" on:click={onDismiss}>稍后</button>
        </div>
      </div>
      {#if state.notes}
        <div class="notes">{state.notes}</div>
      {/if}
    {:else if state.status === 'downloading'}
      <div class="row">
        <span class="msg">
          ⬇️ 下载 v{state.version}
          <span class="bytes">
            {fmt(state.downloaded)}{#if state.total} / {fmt(state.total)}{/if}
          </span>
        </span>
      </div>
      <div class="progress">
        <div class="bar" style="width: {pct == null ? 0 : pct}%"></div>
      </div>
    {:else if state.status === 'error'}
      <div class="row">
        <span class="msg err-msg">
          ⚠️ 更新失败：{state.message}
        </span>
        <div class="actions">
          <button class="btn primary" on:click={onApply}>重试</button>
          <button class="btn" on:click={onCloseError}>关闭</button>
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .update-banner {
    background: linear-gradient(180deg, #f0f9ff, #e0f2fe);
    border-bottom: 1px solid #bae6fd;
    padding: 8px 12px;
    font-size: 12px;
    color: #0c4a6e;
  }
  .update-banner.err {
    background: linear-gradient(180deg, #fef2f2, #fee2e2);
    border-bottom-color: #fca5a5;
    color: #7f1d1d;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-height: 24px;
  }
  .msg {
    flex: 1;
    min-width: 0;
  }
  .form-hint {
    color: #64748b;
    margin-left: 4px;
    font-size: 11px;
  }
  .bytes {
    color: #475569;
    font-variant-numeric: tabular-nums;
    margin-left: 6px;
  }
  .notes {
    margin-top: 4px;
    color: #475569;
    font-size: 11px;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .btn {
    appearance: none;
    border: 1px solid #cbd5e1;
    background: #fff;
    color: inherit;
    padding: 4px 10px;
    border-radius: 4px;
    font-size: 12px;
    cursor: pointer;
  }
  .btn:hover {
    background: #f1f5f9;
  }
  .btn.primary {
    background: var(--brand, #0284c7);
    color: #fff;
    border-color: var(--brand, #0284c7);
  }
  .btn.primary:hover {
    filter: brightness(0.95);
  }
  .progress {
    margin-top: 6px;
    height: 4px;
    background: rgba(0, 0, 0, 0.06);
    border-radius: 2px;
    overflow: hidden;
  }
  .bar {
    height: 100%;
    background: var(--brand, #0284c7);
    transition: width 100ms linear;
  }
  .err-msg {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
