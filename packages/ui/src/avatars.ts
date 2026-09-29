import type { AVATAR_KEYS } from "@farik/brand";
import architect from "@farik/brand/assets/avatars/architect-256.png";
import developer from "@farik/brand/assets/avatars/developer-256.png";
import extra1 from "@farik/brand/assets/avatars/extra-1-256.png";
import extra2 from "@farik/brand/assets/avatars/extra-2-256.png";
import extra3 from "@farik/brand/assets/avatars/extra-3-256.png";
import extra4 from "@farik/brand/assets/avatars/extra-4-256.png";
import extra5 from "@farik/brand/assets/avatars/extra-5-256.png";
import marketingSpecialist from "@farik/brand/assets/avatars/marketing-specialist-256.png";
import productManager from "@farik/brand/assets/avatars/product-manager-256.png";
import scrumMaster from "@farik/brand/assets/avatars/scrum-master-256.png";

export type AvatarKey = (typeof AVATAR_KEYS)[number];

export const AVATAR_URLS: Record<AvatarKey, string> = {
	"product-manager": productManager,
	"scrum-master": scrumMaster,
	architect,
	developer,
	"marketing-specialist": marketingSpecialist,
	"extra-1": extra1,
	"extra-2": extra2,
	"extra-3": extra3,
	"extra-4": extra4,
	"extra-5": extra5,
};
