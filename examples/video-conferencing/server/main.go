package main

import (
	"context"
	"encoding/json"
	"log"
	"net/http"
	"os"
	"sync"

	"github.com/google/uuid"
	"github.com/gorilla/websocket"
	pb "video-conferencing/gen/slatestreams/v1"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

// SignalingMessage is the envelope for all WebRTC signaling exchanged through SlateStreams.
type SignalingMessage struct {
	Type     string          `json:"type"`               // join, offer, answer, ice-candidate, leave
	SenderID string          `json:"sender_id"`
	TargetID string          `json:"target_id,omitempty"`
	RoomID   string          `json:"room_id,omitempty"`
	Payload  json.RawMessage `json:"payload,omitempty"`
}

// Participant is a connected browser client.
type Participant struct {
	ID   string
	Conn *websocket.Conn
	Room string
	mu   sync.Mutex
}

// Room groups participants and owns the SlateStreams stream subscription for a room.
type Room struct {
	ID           string
	Participants map[string]*Participant
	mu           sync.RWMutex
	cancel       context.CancelFunc
}

// Server holds application state.
type Server struct {
	rooms    map[string]*Room
	mu       sync.RWMutex
	client   pb.StreamServiceClient
	basin    string
	upgrader websocket.Upgrader
}

func main() {
	grpcAddr := envOr("SLATESTREAMS_ADDR", "localhost:50051")
	listenAddr := envOr("LISTEN_ADDR", ":8080")

	conn, err := grpc.NewClient(grpcAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		log.Fatalf("failed to connect to SlateStreams at %s: %v", grpcAddr, err)
	}
	defer conn.Close()

	srv := &Server{
		rooms:  make(map[string]*Room),
		client: pb.NewStreamServiceClient(conn),
		basin:  "video-conferencing",
		upgrader: websocket.Upgrader{
			CheckOrigin: func(r *http.Request) bool { return true },
		},
	}

	http.HandleFunc("/ws", srv.handleWebSocket)

	log.Printf("listening on %s  (SlateStreams: %s)", listenAddr, grpcAddr)
	log.Fatal(http.ListenAndServe(listenAddr, nil))
}

// handleWebSocket upgrades the HTTP connection and manages a single participant.
func (s *Server) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	ws, err := s.upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("upgrade error: %v", err)
		return
	}
	defer ws.Close()

	participant := &Participant{
		ID:   uuid.New().String(),
		Conn: ws,
	}

	_ = participant // TODO: read messages, join/leave rooms, relay signaling
}

// joinRoom adds a participant to a room, creating it and starting the stream
// follower if it doesn't exist yet.
func (s *Server) joinRoom(roomID string, p *Participant) {
	// TODO: create room if needed, start SlateStreams Read follower, add participant
}

// leaveRoom removes a participant and tears down the room when empty.
func (s *Server) leaveRoom(p *Participant) {
	// TODO: remove participant, cancel follower if last one out
}

// appendSignal publishes a signaling message to the room's SlateStreams stream.
func (s *Server) appendSignal(ctx context.Context, roomID string, msg SignalingMessage) error {
	// TODO: marshal msg, call s.client.Append with basin=s.basin, stream="room-"+roomID
	return nil
}

// followRoom reads the room's stream and fans out messages to participants.
func (s *Server) followRoom(ctx context.Context, room *Room) {
	// TODO: call s.client.Read (streaming), unmarshal each record, send to participants via WebSocket
}

// broadcast sends a signaling message to all participants in a room except the sender.
func (s *Server) broadcast(room *Room, msg SignalingMessage) {
	// TODO: iterate room.Participants, skip sender, write JSON to websocket
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
