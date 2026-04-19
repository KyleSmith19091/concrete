export interface SignalingMessage {
  type: "join" | "offer" | "answer" | "ice-candidate" | "leave";
  sender_id: string;
  target_id?: string;
  room_id?: string;
  payload?: unknown;
}

export const WS_URL =
  process.env.NEXT_PUBLIC_WS_URL ?? "ws://localhost:8080/ws";

export const RTC_CONFIG: RTCConfiguration = {
  iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
};
