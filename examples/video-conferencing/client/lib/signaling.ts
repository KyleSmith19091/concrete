// --- HTTP signaling (join/leave) ---

export const API_URL =
  process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";

export const WS_URL =
  process.env.NEXT_PUBLIC_WS_URL ?? "ws://localhost:8080/ws";

export interface JoinResponse {
  participant_id: string;
  room_id: string;
}

export async function joinRoom(roomId: string): Promise<JoinResponse> {
  const res = await fetch(`${API_URL}/signal`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ type: "join", room_id: roomId }),
  });
  if (!res.ok) {
    throw new Error(`join failed: ${res.status} ${await res.text()}`);
  }
  return res.json();
}

export async function leaveRoom(
  roomId: string,
  participantId: string,
): Promise<void> {
  await fetch(`${API_URL}/signal`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      type: "leave",
      sender_id: participantId,
      room_id: roomId,
    }),
  });
}

// --- Binary media frames (WebSocket only) ---

// MIME type for MediaRecorder / MediaSource.
// webm + opus + vp8 has the widest browser support for this pipeline.
export const MEDIA_MIME = "video/webm;codecs=vp8,opus";

/**
 * Media frame binary layout:
 *
 *   Offset  Size  Field
 *   ──────  ────  ─────
 *   0       4     Magic bytes: 0x53 0x4C 0x41 0x54 ("SLAT")
 *   4       1     Frame type (0x01 = video+audio)
 *   5       36    Sender ID (UUID, ASCII, null-padded)
 *   41      ...   Media data (webm chunk)
 */

const MAGIC = new Uint8Array([0x53, 0x4c, 0x41, 0x54]); // "SLAT"
const HEADER_SIZE = 4 + 1 + 36; // 41 bytes

export const FRAME_TYPE_VIDEO = 0x01;

export function encodeMediaFrame(
  senderId: string,
  data: ArrayBuffer,
): ArrayBuffer {
  const encoder = new TextEncoder();
  const idBytes = encoder.encode(senderId.padEnd(36, "\0").slice(0, 36));
  const buf = new ArrayBuffer(HEADER_SIZE + data.byteLength);
  const view = new Uint8Array(buf);
  view.set(MAGIC, 0);
  view[4] = FRAME_TYPE_VIDEO;
  view.set(idBytes, 5);
  view.set(new Uint8Array(data), HEADER_SIZE);
  return buf;
}

export function decodeMediaFrame(buf: ArrayBuffer): {
  senderId: string;
  data: ArrayBuffer;
} {
  const view = new Uint8Array(buf);
  const decoder = new TextDecoder();
  const senderId = decoder.decode(view.slice(5, 41)).replace(/\0/g, "");
  const data = view.slice(HEADER_SIZE).buffer;
  return { senderId, data };
}
