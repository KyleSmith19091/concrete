.PHONY: generate run clean

generate:
	buf generate proto

run: generate
	go run .

clean:
	rm -rf gen/
