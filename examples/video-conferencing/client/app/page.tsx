import { JoinForm } from "./components/join-form";

export default function Home() {
  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="w-full max-w-sm space-y-6 p-6">
        <div className="text-center space-y-2">
          <h1 className="text-2xl font-semibold">SlateStreams Video Chat</h1>
          <p className="text-sm text-zinc-400">
            Join or create a room to start a video call.
          </p>
        </div>
        <JoinForm />
      </div>
    </div>
  );
}
