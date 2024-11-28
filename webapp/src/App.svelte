<script lang="ts">
  import svelteLogo from "./assets/svelte.svg";
  import viteLogo from "./assets/vite.svg";
  import Form from "./lib/Form.svelte";

  import { onMount } from "svelte";

  function handleOnSubmit() {
    console.log("I'm the handleOnSubmit() in App.svelte");
  }

  let config = {
    wifiSsid: "",
    wifiPass: "",
    timeZone: "",
    ledColor: "",
    hours24: false,
  };

  onMount(async () => {
    const res = await fetch("/config");
    config = await res.json();
    console.log(config);
  });

  const saveConfig = async (event: any) => {
    event.preventDefault();
    console.log(event);
    console.log(config);
    const res = await fetch("/config", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(config),
    });
    console.log(await res.text());
  };
</script>

<main>
  <h1>Nixie Clock</h1>

  <form on:submit={saveConfig}>
    <fieldset>
      <legend>WiFi</legend>
      <div>
        <label for="wifiSsid">SSID</label>
        <input
          id="wifiSsid"
          name="wifiSsid"
          type="text"
          bind:value={config.wifiSsid}
        />
      </div>
      <div>
        <label for="wifiPass">Password</label>
        <input
          id="wifiPass"
          name="wifiPass"
          type="text"
          bind:value={config.wifiPass}
        />
      </div>
    </fieldset>

    <fieldset>
      <legend>Clock</legend>
      <label for="timeZone">Time Zone</label>
      <select id="timeZone" name="timeZone" bind:value={config.timeZone}>
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
    </fieldset>

    <fieldset>
      <label for="ledColor">LED Color</label>
      <div class="color-input-container">
        <input
          id="ledColor"
          name="ledColor"
          type="color"
          bind:value={config.ledColor}
        />
      </div>
    </fieldset>

    <fieldset class="hours-container">
      <label for="hours24">24 Hour Time</label>
      <input
        id="hours24"
        name="hours24"
        type="checkbox"
        bind:checked={config.hours24}
      />
    </fieldset>

    <button type="submit">Save</button>
  </form>
</main>

<style>
  main {
    min-width: 800px;
  }

  form {
    margin: 0 auto;
    padding: 20px;
  }

  fieldset {
    margin-bottom: 20px;
    padding: 10px;
    border: none;
  }

  legend {
    font-weight: bold;
    font-size: 2em;
    padding: 0 10px;
  }

  label {
    display: block;
    margin-bottom: 5px;
    font-weight: bold;
    text-align: left;
  }

  input[type="text"],
  input[type="password"],
  select {
    width: 100%;
    padding: 8px;
    margin-bottom: 10px;
    border: 1px solid #ccc;
    border-radius: 5px;
    box-sizing: border-box;
  }

  .color-input-container {
    display: flex;
    align-items: center;
  }

  .hours-container {
    display: flex;
    column-gap: 10px;
  }

  .hours-container label{
    display: inline-block;
  }

  input[type="color"] {
    width: 200px;
    height: 200px;
    padding: 8px;
    margin-bottom: 10px;
    border: 1px solid #ccc;
    border-radius: 5px;
    box-sizing: border-box;
  }

  button[type="submit"] {
    display: block;
    width: 100%;
    padding: 10px;
    background-color: #007bff;
    color: white;
    border: none;
    border-radius: 5px;
    cursor: pointer;
    font-size: 16px;
  }

  button[type="submit"]:hover {
    background-color: #0056b3;
  }
</style>
