package main

import (
	"context"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"os"
	"sync"

	pb "video-conferencing/gen/slatestreams/v1"
	"video-conferencing/model"

	"github.com/google/uuid"
	"github.com/gorilla/websocket"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

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

	http.HandleFunc("/signal", srv.handleSignaling)
	http.HandleFunc("/ws", srv.handleMediaStream)

	log.Printf("listening on %s  (SlateStreams: %s)", listenAddr, grpcAddr)
	log.Fatal(http.ListenAndServe(listenAddr, nil))
}

// handleSignaling is a plain HTTP POST endpoint for join/leave messages.
// Returns the assigned participant ID on join so the client can use it for media frames.
func (s *Server) handleSignaling(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, "could not read body", http.StatusBadRequest)
		return
	}
	defer r.Body.Close()

	var msg model.Message
	if err := json.Unmarshal(body, &msg); err != nil {
		http.Error(w, "invalid JSON", http.StatusBadRequest)
		return
	}

	if err := msg.Valid(); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	switch msg.Type {
	case model.JOIN:
		participantID := uuid.New().String()
		// We don't have a WebSocket yet — register a placeholder participant.
		// The real Conn gets set when they connect to /ws.
		p := &Participant{
			ID:   participantID,
			Room: msg.RoomID,
		}
		s.joinRoom(r.Context(), msg.RoomID, p)

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{
			"participant_id": participantID,
			"room_id":        msg.RoomID,
		})

	case model.LEAVE:
		s.mu.RLock()
		room, found := s.rooms[msg.RoomID]
		s.mu.RUnlock()
		if found {
			room.mu.RLock()
			p, exists := room.Participants[msg.SenderID]
			room.mu.RUnlock()
			if exists {
				s.leaveRoom(p)
			}
		}
		w.WriteHeader(http.StatusOK)

	default:
		http.Error(w, "unsupported message type", http.StatusBadRequest)
	}
}

// handleMediaStream upgrades to WebSocket for binary media frames only.
// The client must pass participant_id and room_id as query params (obtained from POST /signal).
func (s *Server) handleMediaStream(w http.ResponseWriter, r *http.Request) {
	participantID := r.URL.Query().Get("participant_id")
	roomID := r.URL.Query().Get("room_id")
	if participantID == "" || roomID == "" {
		http.Error(w, "participant_id and room_id query params required", http.StatusBadRequest)
		return
	}

	// Look up the participant created during /signal join.
	s.mu.RLock()
	room, roomFound := s.rooms[roomID]
	s.mu.RUnlock()
	if !roomFound {
		http.Error(w, "room not found", http.StatusNotFound)
		return
	}

	room.mu.RLock()
	participant, pFound := room.Participants[participantID]
	room.mu.RUnlock()
	if !pFound {
		http.Error(w, "participant not found — call POST /signal with join first", http.StatusNotFound)
		return
	}

	ws, err := s.upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("upgrade error: %v", err)
		return
	}
	defer ws.Close()

	// Attach the WebSocket connection to the participant.
	participant.mu.Lock()
	participant.Conn = ws
	participant.mu.Unlock()

	log.Printf("participant %s connected media stream for room %s", participantID, roomID)

	// Read loop — binary media frames only.
	for {
		msgType, data, err := ws.ReadMessage()
		if err != nil {
			if websocket.IsCloseError(err, websocket.CloseNormalClosure, websocket.CloseGoingAway) {
				log.Printf("participant %s media stream closed", participantID)
			} else {
				log.Printf("media read error for %s: %v", participantID, err)
			}
			break
		}

		if msgType != websocket.BinaryMessage {
			continue
		}

		s.handleMediaFrame(participant, data)
	}
}

// handleMediaFrame processes a binary media frame (SLAT-prefixed).
func (s *Server) handleMediaFrame(p *Participant, data []byte) {
	frame, err := model.ParseMediaFrame(data)
	if err != nil {
		log.Printf("invalid media frame from %s: %v", p.ID, err)
		return
	}

	// Append the full binary frame (header + media) to the room's stream.
	// Using the raw data preserves the SLAT header so followRoom can decode sender.
	_, err = s.client.Append(context.Background(), &pb.AppendRequest{
		BasinName:  s.basin,
		StreamName: p.Room,
		Data:       data,
	})
	if err != nil {
		log.Printf("append error for %s (frame type %d): %v", p.ID, frame.FrameType, err)
	}
}

// joinRoom adds a participant to a room, creating it if it doesn't exist yet.
func (s *Server) joinRoom(ctx context.Context, roomID string, p *Participant) {
	s.mu.Lock()
	room, found := s.rooms[roomID]
	if !found {
		roomCtx, cancel := context.WithCancel(context.Background())
		room = &Room{
			ID:           roomID,
			Participants: make(map[string]*Participant),
			cancel:       cancel,
		}
		s.rooms[roomID] = room
		go s.followRoom(roomCtx, room)
	}
	s.mu.Unlock()

	room.mu.Lock()
	room.Participants[p.ID] = p
	room.mu.Unlock()

	log.Printf("participant %s joined room %s", p.ID, roomID)
}

// leaveRoom removes a participant and tears down the room when empty.
func (s *Server) leaveRoom(p *Participant) {
	roomID := p.Room

	s.mu.RLock()
	room, found := s.rooms[roomID]
	s.mu.RUnlock()
	if !found {
		return
	}

	// Close the participant's WebSocket if still open.
	p.mu.Lock()
	if p.Conn != nil {
		p.Conn.Close()
		p.Conn = nil
	}
	p.mu.Unlock()

	// Remove from room.
	room.mu.Lock()
	delete(room.Participants, p.ID)
	remaining := len(room.Participants)
	room.mu.Unlock()

	log.Printf("participant %s left room %s (%d remaining)", p.ID, roomID, remaining)

	// If the room is now empty, cancel the follower and clean up.
	if remaining == 0 {
		room.cancel()
		s.mu.Lock()
		delete(s.rooms, roomID)
		s.mu.Unlock()
		log.Printf("room %s closed (empty)", roomID)
	}
}

// followRoom reads the room's stream and fans out media to participants.
func (s *Server) followRoom(ctx context.Context, room *Room) {
	stream, err := s.client.Read(ctx, &pb.ReadRequest{
		BasinName:  s.basin,
		StreamName: room.ID,
		StartSeqNum: 0,
	})
	if err != nil {
		log.Printf("followRoom %s: failed to open read stream: %v", room.ID, err)
		return
	}

	for {
		rec, err := stream.Recv()
		if err != nil {
			if ctx.Err() != nil {
				log.Printf("followRoom %s: context cancelled, stopping", room.ID)
			} else {
				log.Printf("followRoom %s: recv error: %v", room.ID, err)
			}
			return
		}

		// Decode the sender so we can skip echoing back to them.
		if !model.IsMediaFrame(rec.Data) {
			continue
		}
		frame, err := model.ParseMediaFrame(rec.Data)
		if err != nil {
			continue
		}

		// Fan out to every participant except the sender.
		room.mu.RLock()
		for _, p := range room.Participants {
			if p.ID == frame.SenderID {
				continue
			}
			p.mu.Lock()
			if p.Conn != nil {
				if err := p.Conn.WriteMessage(websocket.BinaryMessage, rec.Data); err != nil {
					log.Printf("followRoom %s: write to %s failed: %v", room.ID, p.ID, err)
				}
			}
			p.mu.Unlock()
		}
		room.mu.RUnlock()
	}
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
