<script>
  export let side = 'peer'; // 'self' | 'peer'
  export let text = '';
  export let time = '';
  export let status = '';
</script>

<div class="bubble-wrap" class:self={side === 'self'}>
  <div class="wm-bubble {side}">{text}</div>
  <div class="wm-meta" class:right={side === 'self'}>
    {time}{status && side === 'self' ? ` · ${status}` : ''}
  </div>
</div>

<style>
  .bubble-wrap {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
  }
  .bubble-wrap.self { align-items: flex-end; }
</style>
