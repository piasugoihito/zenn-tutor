import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// Android は WebView が全画面（edge-to-edge）のため、システムバー分の余白を CSS で確保する
if (/Android/i.test(navigator.userAgent)) document.documentElement.classList.add("android");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
