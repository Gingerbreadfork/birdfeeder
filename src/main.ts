import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { watcher } from "./lib/state.svelte";
import { applyRememberedTheme } from "./lib/theme";

applyRememberedTheme();
void watcher.start();

export default mount(App, { target: document.getElementById("app")! });
