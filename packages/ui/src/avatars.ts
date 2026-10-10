import type { AVATAR_KEYS } from "@catervas/brand";
import architect from "@catervas/brand/assets/avatars/architect-256.png";
import developer from "@catervas/brand/assets/avatars/developer-256.png";
import extra1 from "@catervas/brand/assets/avatars/extra-1-256.png";
import extra2 from "@catervas/brand/assets/avatars/extra-2-256.png";
import extra3 from "@catervas/brand/assets/avatars/extra-3-256.png";
import extra4 from "@catervas/brand/assets/avatars/extra-4-256.png";
import extra5 from "@catervas/brand/assets/avatars/extra-5-256.png";
import financeSpecialist from "@catervas/brand/assets/avatars/finance-specialist-256.png";
import marketingSpecialist from "@catervas/brand/assets/avatars/marketing-specialist-256.png";
import productManager from "@catervas/brand/assets/avatars/product-manager-256.png";
import scrumMaster from "@catervas/brand/assets/avatars/scrum-master-256.png";

export type AvatarKey = (typeof AVATAR_KEYS)[number];

export const AVATAR_URLS: Record<AvatarKey, string> = {
	"product-manager": productManager,
	"scrum-master": scrumMaster,
	architect,
	developer,
	"marketing-specialist": marketingSpecialist,
	"finance-specialist": financeSpecialist,
	"extra-1": extra1,
	"extra-2": extra2,
	"extra-3": extra3,
	"extra-4": extra4,
	"extra-5": extra5,
};
