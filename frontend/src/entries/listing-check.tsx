import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ListingCheckScreen } from "../screens/listing-check/ListingCheckScreen";
import "../screens/listing-check/listing-check.css";

// /app/listing-check (求人チェック)。The Rust HTML shell provides <div id="app-root">.
const rootElement = document.getElementById("app-root");
if (rootElement) {
  document.title = "求人チェック";
  createRoot(rootElement).render(
    <StrictMode>
      <ListingCheckScreen />
    </StrictMode>,
  );
}
