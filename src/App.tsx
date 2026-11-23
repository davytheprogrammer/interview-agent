import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './App.css';

function App() {
  const [question, setQuestion] = useState('');
  const [context, setContext] = useState('');
  const [showContext, setShowContext] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [tab, setTab] = useState<'meeting' | 'coding' | 'vision'>('meeting');
  const [apiKey, setApiKey] = useState('');
  const [hasKey, setHasKey] = useState(false);
  const [metaKey, setMetaKey] = useState('');
  const [hasMetaKey, setHasMetaKey] = useState(false);
  const [imageAnswer, setImageAnswer] = useState('');
  const [imageBusy, setImageBusy] = useState(false);
  const [visualCustomPrompt, setVisualCustomPrompt] = useState('');
  const [lastCapturedImage, setLastCapturedImage] = useState<string>('');
  const defaultVisualPrompt = 'Analyze what is visible on this screen and provide the direct, actionable solution.';
  const [answer, setAnswer] = useState('');
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const [audioCheck, setAudioCheck] = useState('');
  const [activeProvider, setActiveProvider] = useState('gemini');
  const [providerKey, setProviderKey] = useState('');
  const [hasProviderKeyMap, setHasProviderKeyMap] = useState<Record<string, boolean>>({});
  const [opacity, setOpacity] = useState<number>(0.85);

  const applyOpacity = async (val: number) => {
    setOpacity(val);
    document.documentElement.style.setProperty('--glass-opacity', String(val));
    document.documentElement.style.setProperty('--card-opacity', String(Math.min(val + 0.05, 1.0)));
    try {
      await invoke('set_window_opacity', { opacity: val });
    } catch {}
  };

  const cycleOpacity = async () => {
    const levels = [1.0, 0.85, 0.70, 0.50];
    const nextIdx = (levels.indexOf(opacity) + 1) % levels.length;
    await applyOpacity(levels[nextIdx]);
  };

  const refreshProviderStatus = async () => {
    try {
      const active = await invoke<string>('get_active_provider');
      setActiveProvider(active);
      const providers = ['gemini', 'together', 'rax', 'meta', 'nvidia'];
      const map: Record<string, boolean> = {};
      for (const p of providers) {
        map[p] = await invoke<boolean>('has_provider_key', { provider: p });
      }
      setHasProviderKeyMap(map);
    } catch {}
  };
  useEffect(() => { void refreshProviderStatus(); }, []);

  const selectProvider = async (p: string) => {
    setActiveProvider(p);
    try {
      await invoke('set_active_provider', { provider: p });
      setMessage(`Switched AI provider to ${p.toUpperCase()}`);
    } catch(e) {
      setMessage(String(e));
    }
  };

  const saveSelectedKey = async () => {
    try {
      await invoke('save_provider_key', { provider: activeProvider, key: providerKey });
      setProviderKey('');
      await refreshProviderStatus();
      setMessage(`${activeProvider.toUpperCase()} key saved securely!`);
    } catch(e) {
      setMessage(String(e));
    }
  };
  const [captureProtection, setCaptureProtection] = useState<{supported:boolean;requested:boolean;description:string}|null>(null);
  const toggleOrientation = async (landscape: boolean) => {
    try {
      await invoke('resize_window', { landscape });
    } catch {}
  };

  useEffect(() => {
    if (showSettings) {
      void toggleOrientation(false);
    } else if (tab === 'meeting' || tab === 'vision') {
      void toggleOrientation(true);
    } else {
      void toggleOrientation(false);
    }
  }, [tab, showSettings]);

  const refreshProtection = () => invoke<{supported:boolean;requested:boolean;description:string}>('capture_protection_status').then(setCaptureProtection).catch(() => setCaptureProtection({supported:false,requested:false,description:'Screen-share protection status is unavailable.'}));
  useEffect(() => { void refreshProtection(); }, []);
  const changeProtection = async (enabled:boolean) => { try { await invoke('set_capture_protection', {enabled}); await refreshProtection(); } catch(e) { setMessage(String(e)); } };
  const [checkingAudio, setCheckingAudio] = useState(false);
  const [recording, setRecording] = useState<'idle'|'starting'|'recording'|'stopping'>('idle');
  const [captureMode, setCaptureMode] = useState<'microphone'|'computer'>('microphone');
  const [meetingSuggestion, setMeetingSuggestion] = useState('');
  const [meetingText, setMeetingText] = useState('');
  const [meetingProgress, setMeetingProgress] = useState('Waiting for speech');
  const [transcriptionReady, setTranscriptionReady] = useState(true);
  const [partial, setPartial] = useState('');
  useEffect(() => { let disposed = false; let unlisten: (() => void) | undefined; let unlistenAnswer: (() => void) | undefined; void Promise.all([listen<{event:string;text:string;mode:string}>('microphone-transcription', ({payload}) => { if (disposed) return; if (payload.event === 'started') { setRecording(old => old === 'stopping' ? old : 'recording'); if (payload.mode === 'computer') setMeetingProgress('Listening for speech'); } if (payload.event === 'ready' && payload.mode === 'computer') setMeetingProgress('Muse is listening for speech turns'); if (payload.event === 'partial') { setPartial(payload.text); if (payload.mode === 'computer' && payload.text) setMeetingProgress('Speech detected — Muse is preparing the turn'); } if (payload.event === 'turn_complete') { if(payload.mode==='microphone') setQuestion(old=>`${old}${old.trim()?' ':''}${payload.text}`); else setMeetingText(payload.text); setPartial(''); } if (payload.event === 'final') { if (payload.mode === 'microphone') setQuestion(old => `${old}${old.trim() ? ' ' : ''}${payload.text}`); else setMeetingText(payload.text); setPartial(''); } if (payload.event === 'error') { setMessage(payload.text); if (payload.mode === 'computer') setMeetingProgress('Audio error'); } if (payload.event === 'ended') { setRecording('idle'); setPartial(''); if (payload.mode === 'computer') setMeetingProgress('Listening stopped'); } }),listen<{event:string;text:string;utterance?:string}>('meeting-suggestion', ({payload}) => { if (disposed) return; if (payload.event === 'buffering') { setMeetingText(payload.text); setMeetingProgress('Muse completed a speech turn'); } if (payload.event === 'processing') { setMeetingProgress('Preparing a draft…'); setMessage(''); } if (payload.event === 'answer') { setMeetingSuggestion(payload.text); setMeetingProgress('Draft ready — still listening'); } if (payload.event === 'no_question') setMeetingProgress('Listening for more speech'); if (payload.event === 'error') { setMessage(payload.text); setMeetingProgress('AI request failed — still listening'); } })]).then(([a,b]) => { if (disposed) { a(); b(); } else { unlisten = a; unlistenAnswer = b; setTranscriptionReady(true); } }).catch(() => setMessage('Could not receive transcription events.')); return () => { disposed = true; unlisten?.(); unlistenAnswer?.(); void invoke('stop_transcription').catch(() => {}); }; }, []);
  const startRecording = async (mode: 'microphone'|'computer') => { setMessage(''); setPartial(''); setCaptureMode(mode); if (mode === 'computer') { setMeetingText(''); setMeetingSuggestion(''); setMeetingProgress('Starting audio…'); try { await invoke('ensure_capture_protection'); } catch (e) { /* non-critical, continue anyway */ } } setRecording('starting'); try { await invoke('start_transcription', {mode}); } catch (e) { setRecording('idle'); setMessage(String(e)); } };
  const stopRecording = async () => { setRecording('stopping'); try { await invoke('stop_transcription'); } catch (e) { setMessage(String(e)); } };
  const testComputerAudio = async () => { setCheckingAudio(true); setAudioCheck('Checking computer-audio access…'); try { setAudioCheck(await invoke<string>('test_system_audio_access')); } catch(e) { setAudioCheck(String(e)); } finally { setCheckingAudio(false); } };
  const testMicrophone = async () => { setCheckingAudio(true); setAudioCheck('Testing microphone access…'); try { if (audio?.native_microphone) { setAudioCheck(await invoke<string>('test_native_microphone')); } else { if (!navigator.mediaDevices?.getUserMedia) throw new Error('Microphone access is unavailable in this desktop webview. Check system permissions and your audio device.'); await invoke('arm_microphone_test'); const stream = await navigator.mediaDevices.getUserMedia({audio:true,video:false}); const device = stream.getAudioTracks()[0]?.label || 'Default microphone'; stream.getTracks().forEach(track => track.stop()); setAudioCheck(`Microphone access granted (${device}). Capture stopped; no audio was saved. Transcription is not connected.`); } } catch(e) { const error = e as Error; setAudioCheck(error.name === 'NotAllowedError' ? 'Microphone access was denied by the webview or operating system. Check the system audio permissions and try again.' : error.name === 'NotFoundError' ? 'No microphone found. Connect one and try again.' : `Microphone access failed: ${error.message || String(e)}`); } finally { await invoke('disarm_microphone_test').catch(() => {}); setCheckingAudio(false); } };
  const refreshKey = () => invoke<boolean>('has_api_key').then(setHasKey).catch(() => setMessage('Secure credential storage is unavailable.'));
  const refreshMetaKey = () => invoke<boolean>('has_meta_api_key').then(setHasMetaKey).catch(() => setMessage('Secure credential storage is unavailable.'));
  useEffect(() => { void refreshKey(); }, []);
  useEffect(() => { void refreshMetaKey(); }, []);
  const saveKey = async () => { try { await invoke('save_api_key', { key: apiKey }); setApiKey(''); await refreshKey(); setMessage('Key saved securely on this device.'); } catch (e) { setMessage(String(e)); } };
  const saveMetaKey = async () => { try { await invoke('save_meta_api_key', { key: metaKey }); setMetaKey(''); await refreshMetaKey(); setMessage('Meta key saved securely on this device.'); } catch(e) { setMessage(String(e)); } };
  const captureAndAnalyze = async (customPrompt?: string) => {
    setMessage('');
    setImageBusy(true);
    try {
      const data = await invoke<string>('capture_screen_region');
      setLastCapturedImage(data);
      const q = (customPrompt || visualCustomPrompt || defaultVisualPrompt).trim();
      const result = await invoke<string>('analyze_image', { question: q, imageDataUrl: data });
      setImageAnswer(result);
    } catch(e) {
      setMessage(String(e));
    } finally {
      setImageBusy(false);
    }
  };

  const refineLastCapture = async (promptOverride?: string) => {
    const q = (promptOverride || visualCustomPrompt).trim();
    if (!q) {
      setMessage('Type a question or refinement first.');
      return;
    }
    setMessage('');
    setImageBusy(true);
    try {
      let result = '';
      if (lastCapturedImage) {
        result = await invoke<string>('analyze_image', { question: q, imageDataUrl: lastCapturedImage });
      } else {
        const data = await invoke<string>('capture_screen_region');
        setLastCapturedImage(data);
        result = await invoke<string>('analyze_image', { question: q, imageDataUrl: data });
      }
      setImageAnswer(result);
      setVisualCustomPrompt('');
    } catch(e) {
      setMessage(String(e));
    } finally {
      setImageBusy(false);
    }
  };
  const ask = async (test = false) => { setBusy(true); setMessage(''); if (!test) setAnswer(''); try { const result = await invoke<string>('ask_nvidia', { question: test ? 'Reply with OK.' : question, context: test ? '' : context, mode: test ? 'meeting' : 'coding' }); if (test) setMessage('Connection successful.'); else setAnswer(result); } catch (e) { setMessage(String(e)); } finally { setBusy(false); } };
  const [audio, setAudio] = useState<{available: boolean; description: string; native_microphone: boolean} | null>(null);
  const refreshAudio = async () => { setAudio(null); try { setAudio(await invoke<{available: boolean; description: string; native_microphone: boolean}>('system_audio_availability')); } catch { setAudio({available:false,native_microphone:false,description:'Audio source check is unavailable. Open the desktop application to check.'}); } };
  useEffect(() => { void refreshAudio(); }, []);

  return <main className="companion">
    <header>
      <div className="identity"><span className="logo" aria-hidden="true">✳</span><div><strong>Live Companion</strong><small>Your meeting sidekick</small></div></div>
      <div style={{display:'flex',gap:'8px',alignItems:'center'}}>
        <button className="glass-btn" title="Toggle window translucency (100% Solid -> 85% Frosted Glass -> 70% Ghost -> 50% Stealth)" onClick={() => void cycleOpacity()}>
          💧 {Math.round(opacity * 100)}%
        </button>
        <button className="icon" aria-label={showSettings ? 'Close settings' : 'Open settings'} aria-expanded={showSettings} onClick={() => setShowSettings(!showSettings)}>⚙</button>
      </div>
    </header>
    {showSettings ? <section className="settings">
      <div className="section-heading"><div><span className="eyebrow">SETUP</span><h1>Connect your assistant</h1></div><button className="text-button" onClick={() => setShowSettings(false)}>Done</button></div>
      <div className="setup-card" style={{border: '1px solid #3b82f6'}}><span className="setup-icon">AI</span><div>
        <strong>Choose AI Provider (LLM)</strong>
        <p>Select which AI engine generates interview answers. Gemini & Together are ultra-fast (~200ms latency).</p>
        <div style={{display: 'flex', gap: '8px', margin: '8px 0', flexWrap: 'wrap'}}>
          {['gemini', 'together', 'rax', 'meta', 'nvidia'].map(p => (
            <button key={p} style={{background: activeProvider === p ? '#3b82f6' : '#27272a', color: '#fff', padding: '4px 10px', fontSize: '13px', borderRadius: '4px', textTransform: 'uppercase'}} onClick={() => void selectProvider(p)}>
              {p} {hasProviderKeyMap[p] ? '✓' : ''}
            </button>
          ))}
        </div>
        <label htmlFor="provider-key">{activeProvider.toUpperCase()} API Key</label>
        <input id="provider-key" type="password" autoComplete="off" value={providerKey} onChange={e => setProviderKey(e.target.value)} placeholder={hasProviderKeyMap[activeProvider] ? `${activeProvider.toUpperCase()} Key Saved` : `Enter ${activeProvider.toUpperCase()} Key`}/>
        <div className="actions">
          <button onClick={() => void saveSelectedKey()} disabled={!providerKey.trim()}>Save {activeProvider.toUpperCase()} Key</button>
          <button onClick={async () => {
            setMessage(`Testing ${activeProvider.toUpperCase()} latency...`);
            const start = performance.now();
            try {
              await invoke('ask_nvidia', { question: 'Say Hello in one word.', context: '', mode: 'meeting' });
              const ms = Math.round(performance.now() - start);
              setMessage(`⚡ ${activeProvider.toUpperCase()} round-trip latency: ${ms}ms!`);
            } catch(e) {
              setMessage(`Connection test error: ${String(e)}`);
            }
          }} disabled={!hasProviderKeyMap[activeProvider]}>⚡ Test {activeProvider.toUpperCase()} Latency</button>
        </div>
      </div><span className="badge">{hasProviderKeyMap[activeProvider] ? 'Ready' : 'Needs Key'}</span></div>
      <div className="setup-card"><span className="setup-icon">01</span><div><strong>NVIDIA AI</strong><p>integrate.api.nvidia.com · poolside/laguna-xs-2.1</p><label htmlFor="api-key">NVIDIA API key</label><input id="api-key" type="password" autoComplete="off" value={apiKey} onChange={e => setApiKey(e.target.value)} placeholder={hasKey ? 'Key saved' : 'Enter your API key'}/><div className="actions"><button onClick={() => void saveKey()} disabled={!apiKey.trim() || busy}>Save key</button><button onClick={() => void ask(true)} disabled={!hasKey || busy}>Test connection</button>{hasKey && <button disabled={busy} onClick={async () => { try { await invoke('delete_api_key'); await refreshKey(); setMessage('Key deleted.'); } catch(e) { setMessage(String(e)); } }}>Delete key</button>}</div></div><span className="badge">{hasKey ? 'Key saved' : 'Not connected'}</span></div>
      <div className="setup-card"><span className="setup-icon">02</span><div><strong>Meta Muse</strong><p>Muse Spark 1.3 Contributor for interview answers & vision · Muse Voice Transcribe for live speech</p><label htmlFor="meta-api-key">Meta Model API key</label><input id="meta-api-key" type="password" autoComplete="off" value={metaKey} onChange={e=>setMetaKey(e.target.value)} placeholder={hasMetaKey?'Key saved':'Enter Meta API key'}/><div className="actions"><button onClick={()=>void saveMetaKey()} disabled={!metaKey.trim()}>Save Meta key</button><button onClick={async()=>{
        setMessage('Pinging Meta Muse API...');
        const start = performance.now();
        try {
          await invoke('ask_nvidia', { question: 'Ping', context: '', mode: 'meeting' });
          const ms = Math.round(performance.now() - start);
          setMessage(`⚡ Meta Muse Latency: ${ms}ms round-trip`);
        } catch(e) {
          setMessage(`Meta ping failed: ${String(e)}`);
        }
      }} disabled={!hasMetaKey}>⚡ Test Latency</button>{hasMetaKey && <button onClick={async()=>{try{await invoke('delete_meta_api_key');await refreshMetaKey();setMessage('Meta key deleted.')}catch(e){setMessage(String(e))}}}>Delete Meta key</button>}</div><p>Keys are stored in the operating system credential store.</p></div><span className="badge">{hasMetaKey?'Key saved':'Not connected'}</span></div>
      <div className="setup-card"><span className="setup-icon">03</span><div><strong>Audio access</strong><p>{audio?.description ?? 'Checking available sources…'} Test computer audio or request microphone access. Each test stops immediately and saves no audio.</p><div className="actions"><button onClick={() => void refreshAudio()} disabled={checkingAudio || recording !== 'idle'}>Refresh devices</button><button onClick={() => void testComputerAudio()} disabled={!audio?.available || checkingAudio || recording !== 'idle'}>Test computer audio</button><button onClick={() => void testMicrophone()} disabled={checkingAudio || recording !== 'idle'}>Test microphone</button></div>{audioCheck && <p role="status">{audioCheck}</p>}<p>Muse Voice Transcribe receives live audio during capture and detects completed speech turns. Completed meeting turns are sent to the text assistant for suggestions.</p></div><span className="badge">{audio?.available ? 'Source found' : 'Unavailable'}</span></div>
      <div className="setup-card"><span className="setup-icon">03</span><div><strong>Screen sharing</strong><p>{captureProtection?.description ?? 'Checking screen-share protection…'}</p>{captureProtection?.supported && <label className="capture-toggle"><input type="checkbox" checked={captureProtection.requested} onChange={e => void changeProtection(e.target.checked)}/><span className="switch-track"><span className="switch-thumb"></span></span><span className="switch-label">Request OS capture exclusion</span></label>}</div><span className="badge">{captureProtection?.supported ? (captureProtection.requested ? 'Requested' : 'Off') : 'Unavailable'}</span></div>
      <div className="setup-card"><span className="setup-icon">04</span><div><strong>Window Translucency & Glass</strong><p>Adjust window transparency so you can see your code, slides, or meeting through the companion while reading answers.</p>
        <div style={{display:'flex',gap:'8px',marginTop:'8px',flexWrap:'wrap'}}>
          {[1.0, 0.85, 0.70, 0.50].map(val => (
            <button key={val} style={{background: opacity === val ? '#3b82f6' : '#27272a', color: '#fff', padding: '5px 12px', fontSize: '12px', borderRadius: '6px'}} onClick={() => void applyOpacity(val)}>
              {val === 1.0 ? '100% Solid' : val === 0.85 ? '85% Frosted' : val === 0.70 ? '70% Ghost' : '50% Stealth'}
            </button>
          ))}
        </div>
      </div><span className="badge">{Math.round(opacity * 100)}%</span></div>
      <p className="privacy">While capture is active, live audio is streamed to Meta for transcription; completed meeting speech is sent to your configured text assistant. Images are sent to Meta only when you select one for analysis. No raw audio is saved.</p>{recording !== 'idle' && <button className="listen" onClick={() => void stopRecording()} disabled={recording === 'stopping'}>■ Stop capture</button>}{message && <p role="status" className="message">{message}</p>}
    </section> : <>
      <div className="statusline" role="status" aria-live="polite"><span className={'status-dot ' + (recording === 'recording' ? 'live' : '')}/> <span>{recording === 'recording' ? (captureMode === 'computer' ? 'Listening to computer audio' : 'Microphone recording') : recording === 'starting' ? 'Starting audio…' : recording === 'stopping' ? 'Stopping audio…' : 'Not listening'}</span>{recording !== 'idle' ? <button className="text-button" onClick={() => void stopRecording()} disabled={recording === 'stopping'}>Privacy stop</button> : <button className="text-button" onClick={() => setShowSettings(true)}>Set up audio & AI →</button>}</div>
      <nav className="tabs" aria-label="Assistant area"><button className={tab === 'meeting' ? 'selected' : ''} aria-current={tab === 'meeting' ? 'page' : undefined} onClick={() => setTab('meeting')}>◉ &nbsp; Meeting</button><button className={tab === 'coding' ? 'selected' : ''} aria-current={tab === 'coding' ? 'page' : undefined} onClick={() => setTab('coding')}>⌘ &nbsp; Coding</button><button className={tab === 'vision' ? 'selected' : ''} aria-current={tab === 'vision' ? 'page' : undefined} onClick={() => setTab('vision')}>👁 &nbsp; Visual Monitor</button></nav>
      {tab === 'meeting' ?
      <section className="body meeting-landscape">
        {/* Top-level prominent suggestion banner right at eye level beneath the camera */}
        <div className="teleprompter-callout">
          <div className="teleprompter-header">
            <span className="teleprompter-badge">AI REAL-TIME DRAFT</span>
            <span className="teleprompter-status">{meetingProgress}</span>
            <div style={{display: 'flex', gap: '8px', alignItems: 'center'}}>
              <button
                className="listen-pill vision-snap-btn"
                title="Silent Screen Sense (Captures screen silently in RAM and asks AI without focus loss)"
                disabled={imageBusy}
                onClick={async () => {
                  setMessage('Silently scanning screen in memory...');
                  try {
                    const data = await invoke<string>('capture_screen_region');
                    setImageBusy(true);
                    const prompt = meetingText
                      ? `The interviewer asked: "${meetingText}". Using what is visible on screen, provide the direct answer.`
                      : 'Analyze what is shown on screen and provide the direct, actionable solution.';
                    const res = await invoke<string>('analyze_image', { question: prompt, imageDataUrl: data });
                    setMeetingSuggestion(res);
                    setMeetingProgress('Visual answer ready');
                  } catch(e) {
                    setMessage(String(e));
                  } finally {
                    setImageBusy(false);
                  }
                }}
              >
                {imageBusy ? '👁 Scanning…' : '👁 Visual Snap'}
              </button>
              {recording === 'idle' ? (
                <button className="listen-pill" disabled={!audio?.available || !hasMetaKey} onClick={() => void startRecording('computer')}>▶ Start Listening</button>
              ) : (
                <button className="listen-pill active" onClick={() => void stopRecording()}>■ Stop Capture</button>
              )}
            </div>
          </div>
          <div className="teleprompter-text">
            {meetingSuggestion || <span className="teleprompter-placeholder">Listening for interviewer questions... Suggested answers appear here directly below your webcam so your eye contact remains natural. Use "👁 Visual Snap" if the interviewer shares a diagram, slide, or code.</span>}
          </div>
          {(partial || meetingText) && (
            <div className="teleprompter-heard">
              <strong>Heard:</strong> {partial || meetingText}
            </div>
          )}
        </div>
      </section> : tab === 'coding' ? <section className="body">
        <span className="eyebrow">CODING ASSISTANT</span><h1>Ask about<br/><em>your code.</em></h1><p className="intro">Type a question or record one using your microphone. Speech is transcribed on this computer.</p>
        <label htmlFor="question">Coding question</label><textarea id="question" value={question} onChange={e => setQuestion(e.target.value)} placeholder="How do I fix this error?" rows={3}/>
        {recording === 'idle' ? <button className="context-toggle" disabled={!transcriptionReady} onClick={() => void startRecording('microphone')}>● &nbsp; Record a question</button> : <button className="context-toggle" onClick={() => void stopRecording()} disabled={recording === 'stopping'}>■ &nbsp; {recording === 'stopping' ? 'Stopping…' : 'Stop capture'}</button>}{partial && captureMode === 'microphone' && <p className="source" aria-live="polite">Hearing: {partial}</p>}
        <button className="context-toggle" aria-expanded={showContext} onClick={() => setShowContext(!showContext)}>{showContext ? '−' : '+'} {showContext ? 'Hide code or context' : 'Add code or context'}</button>
        {showContext && <><label htmlFor="context">Code or context</label><textarea id="context" value={context} onChange={e => setContext(e.target.value)} placeholder="Paste code or an error message…" rows={4}/></>}
        <button className="ask" disabled={!hasKey || !question.trim() || busy} onClick={() => void ask()}>{busy ? 'Thinking…' : 'Ask coding question'} <span>↗</span></button>
        <p className="source">Your question and context are sent to the configured text assistant when you ask. Review generated code before using it.</p>
        {message && <p role="alert" className="message">{message}</p>}{answer && <div className="callout"><span aria-hidden="true">✳</span><p><strong>AI-generated answer</strong><br/>{answer}</p></div>}
        {!hasKey && <div className="callout"><span aria-hidden="true">✳</span><p><strong>Setup required.</strong><br/>Add your NVIDIA key to get answers. <button className="inline" onClick={() => setShowSettings(true)}>View setup</button></p></div>}
      </section> : <section className="body vision-landscape">
        <div className="vision-callout-card">
          <div className="vision-header-row">
            <div className="vision-meta">
              <span className="vision-badge">AI VISUAL TELEPROMPTER</span>
              <span className="vision-status-text">
                {imageBusy ? '⚡ Analyzing in RAM…' : imageAnswer ? 'Visual solution ready' : 'Ready to capture screen'}
              </span>
            </div>
            <div className="vision-controls">
              <div className="vision-pills">
                <button className="vision-pill-btn" disabled={imageBusy} onClick={() => void captureAndAnalyze('Solve the coding problem or challenge on screen. Provide the complete optimal code and direct solution.')}>🎯 Solve</button>
                <button className="vision-pill-btn" disabled={imageBusy} onClick={() => void captureAndAnalyze('Identify the bug, syntax issue, or error in the code on screen, and provide the fix.')}>🔍 Debug</button>
                <button className="vision-pill-btn" disabled={imageBusy} onClick={() => void captureAndAnalyze('Explain the architecture, system design, or diagram shown on screen.')}>🏗️ Design</button>
                <button className="vision-pill-btn" disabled={imageBusy} onClick={() => void captureAndAnalyze('Analyze the algorithm on screen. Give the optimal time and space complexity.')}>⏱️ Complexity</button>
              </div>
              <button className="snap-action-btn" disabled={imageBusy} onClick={() => void captureAndAnalyze()}>
                {imageBusy ? '⚡ Scanning…' : '⚡ Snap Screen'}
              </button>
            </div>
          </div>
          <div className="vision-teleprompter-text">
            {imageBusy ? (
              <span className="vision-loading-text">
                <span className="status-dot live" style={{display:'inline-block',marginRight:'8px'}} />
                Silently capturing screen framebuffer in RAM… Analyzing with Vision AI…
              </span>
            ) : imageAnswer ? (
              <>
                <button className="vision-copy-floating" onClick={() => { navigator.clipboard.writeText(imageAnswer); setMessage('Copied to clipboard!'); }}>Copy</button>
                {imageAnswer}
              </>
            ) : (
              <span className="teleprompter-placeholder">
                Click "⚡ Snap Screen" or type a question below to silently analyze your screen in ~300ms without window focus loss or screen flickering. Direct answers appear here at eye level.
              </span>
            )}
          </div>

          {/* Refinement & Custom Question Input Bar */}
          <div className="vision-input-bar">
            <input
              type="text"
              className="vision-input-field"
              value={visualCustomPrompt}
              onChange={e => setVisualCustomPrompt(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  if (lastCapturedImage) {
                    void refineLastCapture();
                  } else {
                    void captureAndAnalyze();
                  }
                }
              }}
              placeholder={imageAnswer ? "Ask a follow-up to refine (e.g. 'Optimize space to O(1)', 'Convert to Python')..." : "Ask a specific question about what's on screen..."}
              disabled={imageBusy}
            />
            {imageAnswer && (
              <button
                className="vision-refine-btn"
                disabled={imageBusy || !visualCustomPrompt.trim()}
                onClick={() => void refineLastCapture()}
                title="Refine the current solution without re-snapping the screen"
              >
                💬 Refine
              </button>
            )}
            <button
              className="vision-snap-ask-btn"
              disabled={imageBusy}
              onClick={() => void captureAndAnalyze()}
              title="Capture a new screen view and ask your question"
            >
              ⚡ Snap & Ask
            </button>
          </div>

          {/* Quick refinement chips when an answer is present */}
          {imageAnswer && !imageBusy && (
            <div className="vision-chips-row">
              <span style={{fontSize:'10px',color:'#64748b',fontWeight:700,letterSpacing:'0.05em'}}>QUICK REFINE:</span>
              <button className="vision-chip" onClick={() => void refineLastCapture('Optimize the time and space complexity of this solution to the absolute optimal.')}>✨ Optimize</button>
              <button className="vision-chip" onClick={() => void refineLastCapture('Rewrite this solution in clean Python 3.')}>🐍 In Python</button>
              <button className="vision-chip" onClick={() => void refineLastCapture('Rewrite this solution in modern TypeScript / JavaScript.')}>⚡ In TypeScript</button>
              <button className="vision-chip" onClick={() => void refineLastCapture('What are the critical edge cases (e.g. null, empty, duplicates, overflow) to watch out for?')}>🛡️ Edge Cases</button>
              <button className="vision-chip" onClick={() => void refineLastCapture('Give me a 3-sentence natural spoken summary of this solution that I can speak aloud to the interviewer.')}>🗣️ Spoken Pitch</button>
            </div>
          )}
        </div>
        {message && <p role="alert" className="message" style={{margin:'6px 0 0',textAlign:'center'}}>{message}</p>}
      </section>}
      <footer><span>{recording === 'recording' ? (captureMode === 'computer' ? '● Computer audio active' : '● Microphone active') : recording === 'idle' ? '◌ No audio captured' : '◌ Audio changing state'}</span><span>{captureProtection?.supported && captureProtection.requested ? 'Capture exclusion requested — verify' : 'Window may appear in screen shares'}</span></footer>
    </>}
  </main>;
}
export default App;
