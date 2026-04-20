package model

import "errors"

var (
	// MagicHeader is the 4-byte prefix on every media frame: "SLAT" (0x53 0x4C 0x41 0x54).
	// Check this before parsing to distinguish binary media from JSON signaling.
	MagicHeader = []byte{0x53, 0x4C, 0x41, 0x54}

	ErrNotMediaFrame = errors.New("missing SLAT magic header")
	ErrFrameTooShort = errors.New("media frame too short")
)

const (
	FRAME_TYPE_VIDEO = 0x01

	magicSize   = 4  // "SLAT"
	typeSize    = 1  // frame type byte
	senderSize  = 36 // UUID ASCII, null-padded
	headerSize  = magicSize + typeSize + senderSize // 41
)

type MediaFrame struct {
	FrameType uint8
	SenderID  string
	Media     []byte
}

// IsMediaFrame checks whether data starts with the SLAT magic bytes.
func IsMediaFrame(data []byte) bool {
	return len(data) >= magicSize &&
		data[0] == MagicHeader[0] &&
		data[1] == MagicHeader[1] &&
		data[2] == MagicHeader[2] &&
		data[3] == MagicHeader[3]
}

// ParseMediaFrame decodes a binary media frame.
//
//	Layout: [4 magic "SLAT"] [1 type] [36 sender_id] [... media data]
func ParseMediaFrame(data []byte) (MediaFrame, error) {
	if !IsMediaFrame(data) {
		return MediaFrame{}, ErrNotMediaFrame
	}
	if len(data) < headerSize {
		return MediaFrame{}, ErrFrameTooShort
	}

	frameType := data[4]
	senderID := trimNull(string(data[5:41]))
	media := data[headerSize:]

	return MediaFrame{
		FrameType: frameType,
		SenderID:  senderID,
		Media:     media,
	}, nil
}

func trimNull(s string) string {
	for i := range len(s) {
		if s[i] == 0 {
			return s[:i]
		}
	}
	return s
}
