import { useParams } from "react-router";
import { Navigate } from "../app/links.tsx";

/** `/chats/:id`, the address a person guesses for one chat: `/channel/:id` is where it opens. */
export function ChatsAlias() {
	const { id } = useParams();
	return <Navigate to={`/channel/${id ?? ""}`} replace />;
}
