#!/usr/bin/env python3
"""Explicitly started live audio capture streamed to Meta Muse Voice Transcribe."""
import asyncio, json, os, subprocess, sys, threading
import websockets

stop=threading.Event(); capture=None

def send(event,text=''):
    print(json.dumps({'event':event,'text':text}),flush=True)

def control():
    global capture
    sys.stdin.readline(); stop.set()
    if capture is not None and capture.poll() is None:
        capture.terminate()

threading.Thread(target=control,daemon=True).start()

def launch_capture(source):
    if sys.platform=='win32':
        helper=os.path.join(os.path.dirname(__file__),'platform','windows','audio.py')
        return subprocess.Popen([sys.executable,'-u',helper,'capture',source],stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
    return subprocess.Popen(['parec','--device='+source,'--raw','--format=s16le','--rate=16000','--channels=1','--latency-msec=50'],stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)

async def run(source):
    global capture
    key=os.environ.get('META_MODEL_API_KEY')
    if not key: raise RuntimeError('Meta Model API key is missing.')
    capture=launch_capture(source)
    send('started')
    uri='wss://api.meta.ai/v1/asr/realtime'
    async with websockets.connect(uri,open_timeout=20,ping_interval=20) as ws:
        await ws.send(json.dumps({'authorization':{'accessToken':'Bearer '+key},'model':'muse-voice-transcribe-1.0','mode':'ENDPOINTING','endpointingConfig':{'silenceDurationMs':500},'audioEncoding':'PCM_16KHZ','partialMode':'CUMULATIVE','emitAudioProgress':False}))
        ack=json.loads(await asyncio.wait_for(ws.recv(),timeout=10))
        if 'sessionId' not in ack: raise RuntimeError('Muse Voice Transcribe handshake failed.')
        send('ready')
        async def audio_sender():
            while not stop.is_set():
                data=await asyncio.to_thread(capture.stdout.read,1024)
                if not data: break
                await ws.send(data)
                await asyncio.sleep(0.02)
            await ws.send(json.dumps({'type':'endStream'}))
        sender=asyncio.create_task(audio_sender())
        try:
            async for message in ws:
                if isinstance(message,bytes): continue
                event=json.loads(message); kind=event.get('type')
                if kind=='transcript': send('partial',event.get('transcript',''))
                elif kind=='speechComplete': send('turn_complete',event.get('transcript',''))
                elif kind=='error': send('error','Muse Voice Transcribe reported an error. Check API access and connection.')
                elif kind=='audioProgress': send('activity')
                if stop.is_set() and sender.done(): break
        finally:
            if not sender.done(): sender.cancel()
            await asyncio.gather(sender,return_exceptions=True)
    if not stop.is_set(): send('error','Audio capture or transcription ended unexpectedly.')

try:
    asyncio.run(run(sys.argv[1]))
except Exception:
    send('error','Could not connect to Muse Voice Transcribe or open the selected audio device.')
finally:
    if capture is not None:
        if capture.poll() is None: capture.terminate()
        try: capture.wait(timeout=2)
        except subprocess.TimeoutExpired: capture.kill(); capture.wait()
    send('ended')
