import { serviceLogo } from "./service-logo.ts";

// Decorative: the service's name stands beside it.
export function ServiceLogo({ name }: { name: string }) {
	return <img src={serviceLogo(name)} alt="" width={20} height={20} />;
}
