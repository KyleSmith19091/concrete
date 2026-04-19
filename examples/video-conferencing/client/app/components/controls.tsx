"use client";

interface ControlsProps {
  micEnabled: boolean;
  camEnabled: boolean;
  onToggleMic: () => void;
  onToggleCam: () => void;
  onLeave: () => void;
}

export function Controls({
  micEnabled,
  camEnabled,
  onToggleMic,
  onToggleCam,
  onLeave,
}: ControlsProps) {
  return (
    <div className="flex items-center justify-center gap-4 border-t border-zinc-800 px-6 py-4">
      <button
        onClick={onToggleMic}
        className={`flex h-12 w-12 items-center justify-center rounded-full text-lg transition-colors ${
          micEnabled
            ? "bg-zinc-800 hover:bg-zinc-700"
            : "bg-red-600 hover:bg-red-500"
        }`}
        title={micEnabled ? "Mute mic" : "Unmute mic"}
      >
        {micEnabled ? "🎙" : "🔇"}
      </button>

      <button
        onClick={onToggleCam}
        className={`flex h-12 w-12 items-center justify-center rounded-full text-lg transition-colors ${
          camEnabled
            ? "bg-zinc-800 hover:bg-zinc-700"
            : "bg-red-600 hover:bg-red-500"
        }`}
        title={camEnabled ? "Turn off camera" : "Turn on camera"}
      >
        {camEnabled ? "📷" : "🚫"}
      </button>

      <button
        onClick={onLeave}
        className="flex h-12 w-12 items-center justify-center rounded-full bg-red-600 text-lg hover:bg-red-500 transition-colors"
        title="Leave room"
      >
        ✕
      </button>
    </div>
  );
}
