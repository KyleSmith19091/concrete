package model

import "fmt"

const (
	JOIN          = "join"
	OFFER         = "offer"
	ANSWER        = "answer"
	ICE_CANDIDATE = "ice-candidate"
	LEAVE         = "leave"
)

type Message struct {
	Type     string `json:"type"`
	SenderID string `json:"sender_id"`
	TargetID string `json:"target_id"`
	RoomID   string `json:"room_id"`
	Payload  any    `json:"payload"`
}

func (m Message) Valid() error {
	if !isValidType(m.Type) {
		return fmt.Errorf("invalid message type: %s", m.Type)
	}

	return nil
}

func isValidType(Type string) bool {
	return Type == JOIN || Type == OFFER || Type == ANSWER || Type == ICE_CANDIDATE || Type == LEAVE
}
