<script>
  import Icon from '../Icon.svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { reset } from '../store.js';

  function back() { reset(); }
</script>

<div class="wm-app">
  <SimpleHeader title="" onBack={back} />
  <div
    class="wm-flowpage"
    style="align-items:center; justify-content:center; text-align:center; padding:40px 32px;"
  >
    <div class="badge"><Icon name="shield-check" size={26} stroke={1.6} /></div>
    <h2 style="margin-top:8px; text-align:center; font-size:18px;">会话已结束</h2>
    <div class="desc" style="text-align:center; max-width:320px;">
      会话密钥已从内存清除。
    </div>
    <div class="wm-row" style="justify-content:center; margin-top:8px;">
      <button class="wm-btn primary" on:click={back}>开始新会话</button>
    </div>
  </div>
</div>

<style>
  .badge {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--brand-soft);
    color: var(--brand);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin: 0 auto;
  }
</style>
