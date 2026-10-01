import "@farik/brand/tokens.css";
import "@farik/brand/fonts.css";
import { createRoot } from "react-dom/client";
import { Gallery } from "./Gallery.tsx";

const root = document.getElementById("root");
if (root) createRoot(root).render(<Gallery />);
