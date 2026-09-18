import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Phase 0 placeholder UI. Replaced in Phase 1 by a tray-on-menu + settings window.
 */
export default function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");

  async function greet() {
    setGreetMsg(await invoke("greet", { name }));
  }

  useEffect(() => {
    document.title = "PrivatePrompter";
  }, []);

  return (
    <main className="container">
      <h1>PrivatePrompter</h1>
      <p>
        Privacy-first, on-device prompt optimizer for macOS. Highlight text,
        press <kbd>⌘⇧Space</kbd>, get a better prompt — without your text ever
        leaving your machine.
      </p>

      <p>
        <em>
          Placeholder UI. Phase 1 swaps this for a menu-bar app with an onboarding
          flow and settings window.
        </em>
      </p>

      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          greet();
        }}
      >
        <input
          id="greet-input"
          onChange={(e) => setName(e.currentTarget.value)}
          placeholder="Enter a name..."
        />
        <button type="submit">Test Rust IPC</button>
      </form>
      {greetMsg && <p>{greetMsg}</p>}
    </main>
  );
}