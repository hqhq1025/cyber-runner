<script lang="ts">
  let message = $state("scaffold ok");
</script>

<main>
  <h1>fake run</h1>
  <p class="status">{message}</p>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
  }
  h1 {
    margin: 0;
    font-size: 2rem;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .status {
    margin: 0;
    color: var(--muted);
    font-size: 0.95rem;
  }
</style>
