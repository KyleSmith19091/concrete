"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { VideoGrid } from "./video-grid";
import { Controls } from "./controls";
import {
  WS_URL,
  MEDIA_MIME,
  joinRoom,
  leaveRoom,
  encodeMediaFrame,
  decodeMediaFrame,
} from "@/lib/signaling";

export interface Peer {
  id: string;
  mediaSource: MediaSource;
  sourceBuffer: SourceBuffer | null;
  objectUrl: string;
  pendingChunks: ArrayBuffer[];
}

interface VideoRoomProps {
  roomId: string;
}

const RECORDER_TIMESLICE_MS = 250;

export function VideoRoom({ roomId }: VideoRoomProps) {
  const localVideoRef = useRef<HTMLVideoElement>(null);
  const [localStream, setLocalStream] = useState<MediaStream | null>(null);
  const [peers, setPeers] = useState<Map<string, Peer>>(new Map());
  const [micEnabled, setMicEnabled] = useState(true);
  const [camEnabled, setCamEnabled] = useState(true);
  const [connected, setConnected] = useState(false);
  const [participantId, setParticipantId] = useState<string>("");

  const wsRef = useRef<WebSocket | null>(null);
  const recorderRef = useRef<MediaRecorder | null>(null);
  const peersRef = useRef<Map<string, Peer>>(new Map());

  const sendMediaChunk = useCallback(
    (data: ArrayBuffer) => {
      const ws = wsRef.current;
      if (ws && ws.readyState === WebSocket.OPEN && participantId) {
        ws.send(encodeMediaFrame(participantId, data));
      }
    },
    [participantId],
  );

  // --- Create / destroy peer playback state ---

  function addPeer(id: string): Peer {
    const mediaSource = new MediaSource();
    const objectUrl = URL.createObjectURL(mediaSource);
    const peer: Peer = {
      id,
      mediaSource,
      sourceBuffer: null,
      objectUrl,
      pendingChunks: [],
    };

    mediaSource.addEventListener("sourceopen", () => {
      if (mediaSource.readyState !== "open") return;
      try {
        const sb = mediaSource.addSourceBuffer(MEDIA_MIME);
        peer.sourceBuffer = sb;

        sb.addEventListener("updateend", () => {
          if (peer.pendingChunks.length > 0 && !sb.updating) {
            sb.appendBuffer(peer.pendingChunks.shift()!);
          }
        });
      } catch (err) {
        console.error(`failed to create SourceBuffer for peer ${id}:`, err);
      }
    });

    peersRef.current.set(id, peer);
    setPeers(new Map(peersRef.current));
    return peer;
  }

  function removePeer(id: string) {
    const peer = peersRef.current.get(id);
    if (peer) {
      URL.revokeObjectURL(peer.objectUrl);
      if (
        peer.mediaSource.readyState === "open" &&
        peer.sourceBuffer &&
        !peer.sourceBuffer.updating
      ) {
        try {
          peer.mediaSource.endOfStream();
        } catch {
          // ignore
        }
      }
      peersRef.current.delete(id);
      setPeers(new Map(peersRef.current));
    }
  }

  function feedPeerChunk(senderId: string, chunk: ArrayBuffer) {
    let peer = peersRef.current.get(senderId);
    if (!peer) {
      peer = addPeer(senderId);
    }

    const sb = peer.sourceBuffer;
    if (sb && !sb.updating) {
      try {
        sb.appendBuffer(chunk);
      } catch {
        peer.pendingChunks.push(chunk);
      }
    } else {
      peer.pendingChunks.push(chunk);
    }
  }

  // --- Acquire local media ---

  useEffect(() => {
    if (typeof window === "undefined") return;
    let mounted = true;

    async function getMediaStream() {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({
          audio: true,
          video: true,
        });
        if (mounted) {
          setLocalStream(stream);
          if (localVideoRef.current) {
            localVideoRef.current.srcObject = stream;
          }
        } else {
          stream.getTracks().forEach((track) => track.stop());
        }
      } catch (err) {
        console.error("could not access media devices:", err);
      }
    }

    getMediaStream();

    return () => {
      mounted = false;
    };
  }, [roomId]);

  useEffect(() => {
    return () => {
      localStream?.getTracks().forEach((track) => track.stop());
    };
  }, [localStream]);

  // --- Join room via HTTP, then open WebSocket for media ---

  useEffect(() => {
    if (typeof window === "undefined") return;
    let cancelled = false;

    async function connect() {
      // 1. Join via HTTP — get participant ID from server
      const { participant_id } = await joinRoom(roomId);
      if (cancelled) return;
      setParticipantId(participant_id);

      // 2. Open WebSocket for media, passing IDs as query params
      const wsUrl = `${WS_URL}?participant_id=${encodeURIComponent(participant_id)}&room_id=${encodeURIComponent(roomId)}`;
      const ws = new WebSocket(wsUrl);
      ws.binaryType = "arraybuffer";
      wsRef.current = ws;

      ws.addEventListener("open", () => {
        setConnected(true);
      });

      ws.addEventListener("message", (event) => {
        if (!(event.data instanceof ArrayBuffer)) return;
        const { senderId, data } = decodeMediaFrame(event.data);
        if (senderId !== participant_id) {
          feedPeerChunk(senderId, data);
        }
      });

      ws.addEventListener("close", () => {
        setConnected(false);
      });

      ws.addEventListener("error", (err) => {
        console.error("websocket error:", err);
      });
    }

    connect().catch((err) => {
      console.error("failed to join room:", err);
    });

    return () => {
      cancelled = true;
      wsRef.current?.close();
      wsRef.current = null;
      setConnected(false);
    };
  }, [roomId]);

  // --- MediaRecorder: capture local stream and send chunks ---

  useEffect(() => {
    if (!localStream || !participantId) return;

    if (!MediaRecorder.isTypeSupported(MEDIA_MIME)) {
      console.error("MIME type not supported for recording:", MEDIA_MIME);
      return;
    }

    const recorder = new MediaRecorder(localStream, {
      mimeType: MEDIA_MIME,
      videoBitsPerSecond: 500_000,
    });
    recorderRef.current = recorder;

    recorder.addEventListener("dataavailable", async (e) => {
      if (e.data.size > 0) {
        const buf = await e.data.arrayBuffer();
        sendMediaChunk(buf);
      }
    });

    recorder.start(RECORDER_TIMESLICE_MS);

    return () => {
      recorder.stop();
      recorderRef.current = null;
    };
  }, [localStream, participantId, sendMediaChunk]);

  // --- Controls ---

  function handleToggleMic() {
    localStream?.getAudioTracks().forEach((track) => {
      track.enabled = !track.enabled;
    });
    setMicEnabled((v) => !v);
  }

  function handleToggleCam() {
    localStream?.getVideoTracks().forEach((track) => {
      track.enabled = !track.enabled;
    });
    setCamEnabled((v) => !v);
  }

  function handleLeave() {
    recorderRef.current?.stop();
    wsRef.current?.close();
    localStream?.getTracks().forEach((track) => track.stop());
    setLocalStream(null);
    for (const id of peersRef.current.keys()) {
      removePeer(id);
    }
    // Fire-and-forget leave via HTTP
    if (participantId) {
      leaveRoom(roomId, participantId).catch(() => {});
    }
  }

  return (
    <div className="flex flex-1 flex-col">
      <header className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
        <h1 className="text-lg font-semibold">Room: {roomId}</h1>
        <div className="flex items-center gap-3 text-sm text-zinc-400">
          <span className="flex items-center gap-1.5">
            <span
              className={`inline-block h-2 w-2 rounded-full ${connected ? "bg-green-500" : "bg-zinc-600"}`}
            />
            {connected ? "Connected" : "Disconnected"}
          </span>
          <span>
            {peers.size + 1} participant{peers.size !== 0 ? "s" : ""}
          </span>
        </div>
      </header>

      <VideoGrid localVideoRef={localVideoRef} peers={peers} />

      <Controls
        micEnabled={micEnabled}
        camEnabled={camEnabled}
        onToggleMic={handleToggleMic}
        onToggleCam={handleToggleCam}
        onLeave={handleLeave}
      />
    </div>
  );
}
