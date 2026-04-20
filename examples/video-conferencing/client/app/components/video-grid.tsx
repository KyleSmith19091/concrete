"use client";

import { RefObject } from "react";
import type { Peer } from "./video-room";

interface VideoGridProps {
  localVideoRef: RefObject<HTMLVideoElement | null>;
  peers: Map<string, Peer>;
}

export function VideoGrid({ localVideoRef, peers }: VideoGridProps) {
  return (
    <div className="flex-1 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 p-4 auto-rows-fr">
      {/* Local video */}
      <div className="relative rounded-lg overflow-hidden bg-zinc-900">
        <video
          ref={localVideoRef}
          autoPlay
          muted
          playsInline
          className="h-full w-full object-cover"
        />
        <span className="absolute bottom-2 left-2 rounded bg-black/60 px-2 py-1 text-xs">
          You
        </span>
      </div>

      {/* Remote peers — fed via MediaSource objectUrl */}
      {Array.from(peers.values()).map((peer) => (
        <div key={peer.id} className="relative rounded-lg overflow-hidden bg-zinc-900">
          <video
            autoPlay
            playsInline
            src={peer.objectUrl}
            className="h-full w-full object-cover"
          />
          <span className="absolute bottom-2 left-2 rounded bg-black/60 px-2 py-1 text-xs">
            {peer.id.slice(0, 8)}
          </span>
        </div>
      ))}
    </div>
  );
}
