import { Navigate, useParams } from "react-router";

/** `/chats/:id`, the address a person guesses for one chat: `/channel/:id` is where it opens. */
export function ChatsAlias() {
	const { id } = useParams();
	return <Navigate to={`/channel/${id ?? ""}`} replace />;
}
