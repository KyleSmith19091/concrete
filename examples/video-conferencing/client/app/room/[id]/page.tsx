import { VideoRoom } from "@/app/components/video-room";

export default async function RoomPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  return <VideoRoom roomId={id} />;
}
