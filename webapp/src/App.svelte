<script lang="ts">
  import svelteLogo from './assets/svelte.svg';
  import viteLogo from './assets/vite.svg';
  import Form from "./lib/Form.svelte";

  import { onMount } from 'svelte';

  function handleOnSubmit() {
		console.log("I'm the handleOnSubmit() in App.svelte");
	}

	let weather = {
    hourly: {
      temperature_2m: []
    }
  };

	onMount(async () => {
		const res = await fetch(`https://api.open-meteo.com/v1/forecast?latitude=52.52&longitude=13.41&hourly=temperature_2m`);
		weather = await res.json();
	});  
</script>

<main>
  <h1>Nixie Clock</h1>

  <Form on:submit={handleOnSubmit}>
  </Form>

  <div class="photos">
    {#each weather.hourly.temperature_2m as temp}
      <div>
        <p>{temp}</p>
      </div>
    {:else}
      <!-- this block renders when photos.length === 0 -->
      <p>loading...</p>
    {/each}
  </div>

  <p>
    Check out <a href="https://github.com/sveltejs/kit#readme" target="_blank" rel="noreferrer">SvelteKit</a>, the official Svelte app framework powered by Vite!
  </p>

  <p class="read-the-docs">
    Click on the Vite and Svelte logos to learn more
  </p>
</main>

<style>
  .read-the-docs {
    color: #888;
  }
</style>
