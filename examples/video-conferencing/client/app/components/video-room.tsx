"use client";

import { useEffect, useRef, useState } from "react";
import { VideoGrid } from "./video-grid";
import { Controls } from "./controls";

export interface Peer {
  id: string;
  stream: MediaStream;
  connection: RTCPeerConnection;
}

interface VideoRoomProps {
  roomId: string;
}

export function VideoRoom({ roomId }: VideoRoomProps) {
  const localVideoRef = useRef<HTMLVideoElement>(null);
  const [localStream, setLocalStream] = useState<MediaStream | null>(null);
  const [peers, setPeers] = useState<Map<string, Peer>>(new Map());
  const [micEnabled, setMicEnabled] = useState(true);
  const [camEnabled, setCamEnabled] = useState(true);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    // TODO: acquire local media (getUserMedia)
    // TODO: connect WebSocket to Go server
    // TODO: send join message
    // TODO: handle incoming signaling messages
    // TODO: cleanup on unmount
  }, [roomId]);

  function handleToggleMic() {
    // TODO: toggle audio tracks on localStream
    setMicEnabled((v) => !v);
  }

  function handleToggleCam() {
    // TODO: toggle video tracks on localStream
    setCamEnabled((v) => !v);
  }

  function handleLeave() {
    // TODO: send leave message, close connections, navigate back
  }

  return (
    <div className="flex flex-1 flex-col">
      <header className="flex items-center justify-between border-b border-zinc-800 px-6 py-4">
        <h1 className="text-lg font-semibold">Room: {roomId}</h1>
        <span className="text-sm text-zinc-400">
          {peers.size + 1} participant{peers.size !== 0 ? "s" : ""}
        </span>
      </header>

      <VideoGrid
        localVideoRef={localVideoRef}
        peers={peers}
      />

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
