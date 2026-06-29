// src/components/chat/VoiceInput.tsx
// Voice recording via the WebView2 MediaRecorder API (the same approach used
// by shturman.ai's ChatView). The recorded clip is saved as a media file and
// returned to the parent as an attachment — the agent transcribes it via its
// own STT (Whisper/Groq) when the message is sent, exactly like a voice note
// in a messenger.

import { useState, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Mic, Square, Loader2 } from "lucide-react";

interface VoiceInputProps {
  /** Called with the saved attachment once recording stops. */
  onRecorded: (attachment: { path: string; kind: "audio"; mime: string; name: string }) => void;
}

export function VoiceInput({ onRecorded }: VoiceInputProps) {
  const [recording, setRecording] = useState(false);
  const [processing, setProcessing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const streamRef = useRef<MediaStream | null>(null);

  const start = useCallback(async () => {
    setError(null);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;
      const recorder = new MediaRecorder(stream);
      chunksRef.current = [];
      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) chunksRef.current.push(e.data);
      };
      recorder.onstop = async () => {
        // Stop mic tracks regardless of outcome.
        streamRef.current?.getTracks().forEach((t) => t.stop());
        streamRef.current = null;
        const blob = new Blob(chunksRef.current, { type: "audio/webm" });
        setProcessing(true);
        try {
          const buf = new Uint8Array(await blob.arrayBuffer());
          const path = await invoke<string>("save_media_blob_cmd", {
            data: Array.from(buf),
            ext: "webm",
          });
          onRecorded({
            path,
            kind: "audio",
            mime: "audio/webm",
            name: path.split(/[\\/]/).pop() || "voice.webm",
          });
        } catch (e) {
          setError(String(e));
        } finally {
          setProcessing(false);
        }
      };
      recorderRef.current = recorder;
      recorder.start();
      setRecording(true);
    } catch {
      setError("Нет доступа к микрофону");
    }
  }, [onRecorded]);

  const stop = useCallback(() => {
    recorderRef.current?.stop();
    setRecording(false);
  }, []);

  if (processing) {
    return (
      <button
        type="button"
        className="text-ac-muted p-1.5"
        title="Обработка записи…"
        disabled
      >
        <Loader2 className="w-4 h-4 animate-spin" />
      </button>
    );
  }

  if (recording) {
    return (
      <button
        type="button"
        onClick={stop}
        className="text-ac-red p-1.5 animate-pulse"
        title="Остановить запись"
      >
        <Square className="w-4 h-4" />
      </button>
    );
  }

  return (
    <>
      <button
        type="button"
        onClick={start}
        className="text-ac-muted hover:text-ac-brand p-1.5 transition-colors"
        title="Голосовое сообщение"
      >
        <Mic className="w-4 h-4" />
      </button>
      {error && (
        <span className="text-[10px] text-ac-red absolute -bottom-5 right-0">
          {error}
        </span>
      )}
    </>
  );
}

export default VoiceInput;
