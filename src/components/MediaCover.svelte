<script>
  import Icon from "@iconify/svelte";

  export let thumbnail = ""; // base64
  export let coverUrl = "";  // http(s)
  export let platformIcon = null; // iconify id | null
  export let size = 34;
</script>

<div class="wrap" style:width="{size}px" style:height="{size}px">
  {#if coverUrl}
    <img class="img" src={coverUrl} alt="cover" />
  {:else if thumbnail}
    <img class="img" src={`data:image/jpeg;base64,${thumbnail}`} alt="cover" />
  {:else if platformIcon}
    <Icon icon={platformIcon} width={Math.round(size * 0.65)} color="rgba(255,255,255,0.9)" />
  {/if}
</div>

<style>
  .wrap{
    border-radius: 8px;
    background: rgba(255,255,255,0.08);
    display:flex; align-items:center; justify-content:center;
    flex-shrink:0; overflow:hidden;
  }
  .img{ width:100%; height:100%; object-fit:cover; }
</style>

