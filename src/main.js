const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let greetInputEl;
let greetMsgEl;

async function greet() {
  greetMsgEl.textContent = await invoke("greet", { name: greetInputEl.value });
}

window.addEventListener("DOMContentLoaded", () => {
  greetInputEl = document.querySelector("#greet-input");
  greetMsgEl = document.querySelector("#greet-msg");
  document.querySelector("#greet-form").addEventListener("submit", (e) => {
    e.preventDefault();
    greet();
  });

  // 강제 업데이트 이벤트 수신
  listen("mandatory-update", (event) => {
    const info = event.payload;
    const overlay = document.getElementById("mandatory-update-overlay");
    const versionEl = document.getElementById("update-version");
    const notesEl = document.getElementById("update-notes");

    versionEl.textContent = `v${info.current_version} → v${info.latest_version}`;
    notesEl.textContent = info.release_notes;
    overlay.style.display = "flex";

    document.getElementById("update-btn").addEventListener("click", async () => {
      const btn = document.getElementById("update-btn");
      btn.textContent = "설치 중...";
      btn.disabled = true;
      try {
        await invoke("install_update");
      } catch (e) {
        btn.textContent = "업데이트 설치";
        btn.disabled = false;
        console.error("Update failed:", e);
      }
    });
  });
});
