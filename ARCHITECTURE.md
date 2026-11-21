# Architecture and implementation plan

Live Companion uses a Tauri 2 shell and React/TypeScript. Session lifecycle, audio, transcription, AI, persistence, export, privacy, and diagnostics must become separate modules with typed boundaries and minimal native capabilities.

1. Build the runnable shell and explicit session lifecycle with visible capture state and manual notes.
2. Implement and verify platform-specific microphone discovery, permission and interruption handling. Keep system audio unavailable until a tested implementation exists.
3. Add transcription behind an interface after a transcription service and contract are chosen, with explicit consent for transmission.
4. Add an AI provider interface and OS credential storage once endpoint, model, authorization, request and response schemas, and streaming protocol are supplied. Bound context, timeouts, retries and cancellation.
5. Add SQLite migrations, opt-in history, retention, deletion, review and exports. Test lifecycle, deletion, validation and provider failures.
6. Verify permissions, shortcuts, accessibility and packaging separately on Linux, Windows and macOS.

Current increment: webview microphone capture without recording or transcription, not verified on desktop. Opt-in SQLite history uses versioned initialization and parameterized queries. AI, transcription, secure credentials, device selection, import, retention scheduling and automatic screen-share detection are not implemented.
