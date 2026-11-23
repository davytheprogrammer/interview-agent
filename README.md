# Live Companion (Interview Agent)

**Live Companion** is an ultra-fast, cross-platform AI meeting and interview assistant built for Windows and Linux. It pairs low-latency audio capture and real-time speech transcription with instant, conversational AI suggestions rendered directly below your webcam for effortless eye contact.

---

## ⚡ Key Features

### 1. Multi-Provider AI Engine (Sub-250ms Latency)
Choose your preferred LLM provider directly from Settings with built-in round-trip latency benchmarking:
- **Google Gemini:** `gemini-1.5-flash` (~200ms round-trip)
- **Together AI:** `arize-ai/qwen-2-1.5b-instruct` (Ultra-fast low-latency inference)
- **Rax AI:** `rax-4.0` via `https://www.ai.raxcore.dev/api/v1/chat/completions`
- **Meta Muse:** `Muse Spark 1.3 Contributor`
- **NVIDIA AI:** `poolside/laguna-xs-2.1`

### 2. Landscape Teleprompter Mode (Eye-Contact Alignment)
- **Webcam-level Positioning:** In Meeting mode, the window expands into an 840px × 350px landscape teleprompter bar that sits directly below your webcam.
- **Natural Gaze:** Suggested answers appear at the top in large, scannable typography so you can read without looking away from your interviewer.
- **Auto-Responsive Layout:** Automatically returns to a 440px × 680px portrait layout when switching to Coding, Screenshot, or Settings.

### 3. Utterance Continuation & Turn Stitcher
- **Multi-Part Question Stitching:** If an interviewer pauses mid-sentence or follows up within ~3.5 seconds (e.g., *"Can you introduce yourself..."* followed by *"...and walk me through your background?"*), Live Companion automatically stitches the speech turns together.
- **Unified Responses:** Automatically cancels any partial draft in progress and delivers a complete, comprehensive response addressing all parts of what was asked.

### 4. Screen-Sharing Protection (Invisibility)
- **OS Window Capture Exclusion:** Leverages Windows `SetWindowDisplayAffinity` (`WDA_EXCLUDEFROMCAPTURE`) so the companion window remains invisible on Zoom, Microsoft Teams, Google Meet, and screen recordings.

### 5. High-Performance Audio Capture & Transcription
- **Real-Time Speech Recognition:** Streams incoming audio to Meta Muse Voice Transcribe (`muse-voice-transcribe-1.0`) over low-latency WebSockets.
- **WASAPI Loopback Capture:** Direct hardware loopback with a 512-frame recording buffer for near-zero audio latency.
- **Hardware-Agnostic:** Captures computer audio directly from the speaker output (no virtual cables required).

### 6. Natural Human Interview Flow & Session Memory
- **First-Person Candidate Persona:** Speaks naturally in the first person (`"I"`, `"In my experience"`, `"My approach is..."`) as the candidate. Provides articulate, well-structured answers (2 to 4 fluid sentences) with practical nuance and trade-offs—never robotic one-sentence stubs or AI essays.
- **In-Memory Multi-Turn History:** Automatically maintains dialogue memory across spoken questions and visual screen snaps for context-aware follow-ups throughout the interview.

### 7. Silent Screen Monitor & Translucent Frosted Glass
- **Silent Screen Capture (~12ms):** Native Win32 GDI+ GPU framebuffer capture in memory. No window switching, no focus loss, and no screen dimming—completely invisible to proctoring software.
- **Interview Intelligence for Visual Snaps:** Directly understands live interview context for LeetCode challenges (optimal Big-O, code, explanation), system design diagrams (data flow, bottlenecks), and code debugging (root cause + concise fix).
- **Translucent Ghost Mode:** Native alpha blending (`WS_EX_LAYERED`) and CSS backdrop blur (100% Solid, 85% Frosted Glass, 70% Ghost, 50% Stealth) to see meeting video or code directly through the app.

---

## 📥 Download & Installation

### Windows (x64)
Download the latest binaries from the [**GitHub Releases**](https://github.com/davytheprogrammer/interview-agent/releases/latest):
- **Installer (Recommended):** [`Live.Companion_0.2.0_x64-setup.exe`](https://github.com/davytheprogrammer/interview-agent/releases/download/v0.2.0-windows/Live.Companion_0.2.0_x64-setup.exe)
- **Portable Binary:** [`LiveCompanion.exe`](https://github.com/davytheprogrammer/interview-agent/releases/download/v0.2.0-windows/LiveCompanion.exe)

---

## 🛠️ Building from Source

### Prerequisites
- **Node.js** (v18+)
- **Rust toolchain** (1.75+ or newer: `rustup default stable`)
- **Python** (3.10–3.14) with:
  ```bash
  pip install soundcard numpy websockets cffi
  ```

### Development
1. Clone the repository:
   ```bash
   git clone https://github.com/davytheprogrammer/interview-agent.git
   cd interview-agent
   ```
2. Install npm dependencies:
   ```bash
   npm install
   ```
3. Run the development build:
   ```bash
   npm run tauri dev
   ```

### Production Build
To create a production installer or release binary:
```bash
npx tauri build
```
The output installer will be located in `src-tauri/target/release/bundle/nsis/`.

---

## ⚙️ Configuration & Security

- **Operating System Credential Store:** All API keys (Gemini, Together AI, Rax AI, Meta, NVIDIA) are stored securely using your operating system's native credential store (`keyring` / Windows Credential Manager). Keys are never committed to disk, logs, or databases.
- **Privacy First:** Raw audio is never saved to disk. Audio streaming is only initiated when you explicitly click **Start Listening** and terminates immediately upon **Privacy Stop**.

---

## 🏗️ Architecture

```
┌────────────────────────────────────────────────────────┐
│                   Live Companion UI                    │
│      React 19 + TypeScript + Vite (Tauri 2 Frontend)    │
│  - Landscape Teleprompter Mode (840x350)               │
│  - Multi-Provider Benchmarking & Switcher              │
└───────────────────────────┬────────────────────────────┘
                            │ IPC Commands & Events
┌───────────────────────────▼────────────────────────────┐
│                  Tauri Core (Rust)                     │
│  - Utterance Accumulator & Turn Stitcher               │
│  - Window Capture Exclusion (SetWindowDisplayAffinity) │
│  - Multi-LLM Provider Engine (Gemini/Together/Rax/etc.)│
│  - Secure OS Keyring Storage                           │
└───────────────────────────┬────────────────────────────┘
                            │ Subprocess Stdin/Stdout
┌───────────────────────────▼────────────────────────────┐
│             Audio & Transcription Worker               │
│  - Windows WASAPI Loopback (soundcard / numpy)         │
│  - Meta Muse Voice Transcribe WebSocket Client         │
└────────────────────────────────────────────────────────┘
```

---

## 📄 License
This project is licensed under the MIT License. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for bundled components.
