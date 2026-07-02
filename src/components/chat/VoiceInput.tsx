// src/components/chat/VoiceInput.tsx
// Voice recording via the WebView2 MediaRecorder API. Supports two modes
// (matching shturman.ai):
//   • "live" (default): after recording stops, the clip is transcribed on the
//     fly via Groq/OpenAI Whisper and the recognized text is inserted into the
//     input box — the user reviews/edits it before sending.
//   • "voice-note": the clip is attached as an audio message and sent; the
//     agent transcribes it via its own STT when processing the message.
//
// The mode toggles via the small button next to the mic. If no STT provider is
// configured (no GROQ_API_KEY/OPENAI_API_KEY), live transcription reports the
// error and falls back to attaching a voice note.

import { useState, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Mic, Square, Loader2, Type, AudioLines } from "lucide-react";

interface VoiceInputProps {
  /** Live mode: recognized text appended to the chat input. */
  onTranscribed?: (text: string) => void;
  /** Voice-note mode: clip attached as an audio message. */
  onRecorded?: (attachment: {
    path: string;
    kind: "audio";
    mime: string;
    name: string;
  }) => void;
}

type Mode = "live" | "note";

export function VoiceInput({ onTranscribed, onRecorded }: VoiceInputProps) {
  const [mode, setMode] = useState<Mode>("live");
  const [recording, setRecording] = useState(false);
  const [processing, setProcessing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const speechRef = useRef<any>(null);
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

          if (mode === "live" && onTranscribed) {
            // Transcribe on the fly and drop the text into the input box.
            try {
              const text = await invoke<string>("transcribe_audio_cmd", {
                audioPath: path,
              });
              if (text) {
                onTranscribed(text);
              } else {
                setError("Распознавание вернуло пустой текст");
              }
            } catch (e) {
              const msg = String(e);
              setError(
                msg.includes("No STT provider")
                  ? "Не задан GROQ_API_KEY/OPENAI_API_KEY — переключите на режим голосового сообщения"
                  : msg
              );
            }
          } else if (onRecorded) {
            onRecorded({
              path,
              kind: "audio",
              mime: "audio/webm",
              name: path.split(/[\\/]/).pop() || "voice.webm",
            });
          }
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
      // getUserMedia was blocked/denied (WebView2 permission, or no prompt
      // shown). Fall back to the browser-native Web Speech API for live
      // transcription — no MediaRecorder needed, recognition is on-device via
      // the browser. This is the same approach the old Steersman chat used.
      if (mode === "live" && onTranscribed && webSpeechAvailable()) {
        startWebSpeech(onTranscribed);
        return;
      }
      setError(
        mode === "live"
          ? "Нет доступа к микрофону. Разрешите доступ или переключите на режим голосового сообщения."
          : "Нет доступа к микрофону"
      );
    }
  }, [mode, onTranscribed, onRecorded]);

  /** Whether the browser/WebView2 exposes the Web Speech API. */
  function webSpeechAvailable(): boolean {
    return (
      typeof window !== "undefined" &&
      ("SpeechRecognition" in window || "webkitSpeechRecognition" in window)
    );
  }

  /** Recognize speech via the Web Speech API and feed text to onTranscribed. */
  function startWebSpeech(onText: (text: string) => void) {
    type SR = typeof window & {
      SpeechRecognition?: any;
      webkitSpeechRecognition?: any;
    };
    const w = window as SR;
    const Ctor = w.SpeechRecognition || w.webkitSpeechRecognition;
    if (!Ctor) return;
    const rec = new Ctor();
    rec.lang = "ru-RU";
    rec.interimResults = false;
    rec.maxAlternatives = 1;
    setRecording(true);
    setProcessing(true);
    rec.onresult = (e: any) => {
      const text = e.results?.[0]?.[0]?.transcript ?? "";
      if (text) onText(text);
    };
    rec.onerror = () => setError("Распознавание не удалось");
    rec.onend = () => {
      setRecording(false);
      setProcessing(false);
    };
    speechRef.current = rec;
    rec.start();
  }

  const stop = useCallback(() => {
    if (speechRef.current) {
      speechRef.current.stop();
      speechRef.current = null;
      setRecording(false);
      setProcessing(false);
      return;
    }
    recorderRef.current?.stop();
    setRecording(false);
  }, []);

  // Processing spinner
  if (processing) {
    return (
      <button type="button" className="text-ac-muted p-1.5 relative" disabled title="Обработка…">
        <Loader2 className="w-4 h-4 animate-spin" />
      </button>
    );
  }

  // Recording — stop button
  if (recording) {
    return (
      <button
        type="button"
        onClick={stop}
        className="text-ac-red p-1.5 animate-pulse relative"
        title="Остановить запись"
      >
        <Square className="w-4 h-4" />
      </button>
    );
  }

  // Idle — mode toggle + mic
  return (
    <span className="relative flex items-center">
      <button
        type="button"
        onClick={() => setMode(mode === "live" ? "note" : "live")}
        className="text-ac-faint hover:text-ac-muted p-1"
        title={mode === "live" ? "На лету (нажмите для голосового сообщения)" : "Голосовое сообщение (нажмите для «на лету»)"}
      >
        {mode === "live" ? <Type className="w-3.5 h-3.5" /> : <AudioLines className="w-3.5 h-3.5" />}
      </button>
      <button
        type="button"
        onClick={start}
        className="text-ac-muted hover:text-ac-brand p-1 transition-colors"
        title={mode === "live" ? "Голосовой ввод (распознавание на лету)" : "Записать голосовое сообщение"}
      >
        <Mic className="w-4 h-4" />
      </button>
      {error && (
        <span className="text-[10px] text-ac-red absolute -bottom-5 right-0 whitespace-nowrap">
          {error}
        </span>
      )}
    </span>
  );
}

export default VoiceInput;
