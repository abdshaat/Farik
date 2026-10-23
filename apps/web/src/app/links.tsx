import { useCallback } from "react";
import {
	type LinkProps,
	type NavigateOptions,
	type NavigateProps,
	type NavLinkProps,
	Link as RouterLink,
	Navigate as RouterNavigate,
	NavLink as RouterNavLink,
	type To,
	useParams,
	useNavigate as useRouterNavigate,
} from "react-router";
import { teamPath } from "./teams.ts";

/**
 * `to` in the address of the team the page is in: `/board` is `/t/shop/board` there. An address
 * of setup, All teams or This computer, or one already under `/t/`, stays as it is.
 */
function within(team: string | undefined, to: To): To {
	if (team === undefined) return to;
	if (typeof to === "string") return teamPath(team, to);
	return to.pathname === undefined
		? to
		: { ...to, pathname: teamPath(team, to.pathname) };
}

/** The `Link` of a team's page, which keeps to the team (`app/links.test.ts`). */
export function Link(props: LinkProps) {
	const { team } = useParams();
	return <RouterLink {...props} to={within(team, props.to)} />;
}

/** The `NavLink` of a team's page. */
export function NavLink(props: NavLinkProps) {
	const { team } = useParams();
	return <RouterNavLink {...props} to={within(team, props.to)} />;
}

/** The `Navigate` of a team's page. */
export function Navigate(props: NavigateProps) {
	const { team } = useParams();
	return <RouterNavigate {...props} to={within(team, props.to)} />;
}

/** The `useNavigate` of a team's page: its absolute addresses are the team's. */
export function useNavigate(): (
	to: To | number,
	options?: NavigateOptions,
) => void | Promise<void> {
	const { team } = useParams();
	const navigate = useRouterNavigate();
	return useCallback(
		(to, options) =>
			typeof to === "number"
				? navigate(to)
				: navigate(within(team, to), options),
		[navigate, team],
	);
}
