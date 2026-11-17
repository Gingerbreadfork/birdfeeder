import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { watcher } from "./lib/state.svelte";

void watcher.start();

export default mount(App, { target: document.getElementById("app")! });
