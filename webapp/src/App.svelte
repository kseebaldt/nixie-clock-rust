<script lang="ts">
  import svelteLogo from "./assets/svelte.svg";
  import viteLogo from "./assets/vite.svg";
  import Form from "./lib/Form.svelte";

  import { onMount } from "svelte";

  function handleOnSubmit() {
    console.log("I'm the handleOnSubmit() in App.svelte");
  }

  let config = {
    wifi_ssid: "",
    wifi_pass: "",
    tz: "",
    led_color: 0,
  };

  onMount(async () => {
    const res = await fetch("/config");
    config = await res.json();
  });

  const saveConfig = async (event: any) => {
    event.preventDefault();
    console.log(event);
    console.log(config);
    // const res = await fetch("/config", {
    //   method: "POST",
    //   headers: {
    //     "Content-Type": "application/json",
    //   },
    //   body: JSON.stringify(config),
    // });
    // console.log(await res.json());
  };
</script>

<main>
  <h1>Nixie Clock</h1>

  <form on:submit={saveConfig}>
    <label for="name">SSID</label>
    <input id="wifi_ssid" name="wifi_ssid" bind:value={config.wifi_ssid} />

    <label for="name">Password</label>
    <input id="wifi_pass" name="wifi_pass" bind:value={config.wifi_pass} />

    <label for="tz">Time Zone</label>
    <select id="tz" name="tz" bind:value={config.tz}>
      <option></option>
      <option>US/Alaska</option>
      <option>US/Arizona</option>
      <option>US/Central</option>
      <option>US/East-Indiana</option>
      <option>US/Eastern</option>
      <option>US/Hawaii</option>
      <option>US/Indiana-Starke</option>
      <option>US/Michigan</option>
      <option>US/Mountain</option>
      <option>US/Pacific</option>
      <option>US/Pacific-New</option>
    </select>

    <label for="name">LED Color</label>
    <input
      id="led_color"
      name="led_color"
      type="color"
      bind:value={config.led_color}
    />

    <button type="submit">Save</button>
  </form>
</main>
