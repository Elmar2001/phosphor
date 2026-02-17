# Phosphor

## Project Overview

Phosphor is a desktop GUI for Proxmark3, designed to simplify RFID/NFC card cloning and management. It wraps the Proxmark3 client in a user-friendly "wizard" interface, enabling point-and-click operations for scanning, identifying, and cloning cards.

**Key Features:**
*   **One-click Cloning:** Automated workflows for LF (125 kHz) and HF (13.56 MHz) cards.
*   **Card Support:** Extensive support for LF cards (HID, EM4100, etc.) and HF cards (MIFARE Classic, Ultralight, etc.).
*   **Magic Blank Detection:** Identifies and handles various magic card types (T5577, Gen1a, Gen2, etc.).
*   **Firmware Management:** Built-in tool for flashing Proxmark3 firmware.
*   **Cross-Platform:** Runs on Windows and macOS.

**Architecture:**
*   **Frontend:** Built with React 19, TypeScript, and Tailwind CSS v4. It uses XState v5 for managing complex UI flows and wizard steps.
*   **Backend:** Powered by Rust and Tauri v2. It handles device communication, executes Proxmark3 commands via the external binary, and manages application state.
*   **State Synchronization:** Utilizes a dual state machine architecture where the Rust backend (`WizardMachine`) and the React frontend (XState) stay in sync through Tauri commands.
*   **Database:** Uses SQLite (`rusqlite`) for persistent storage of clone history and saved card data.

## Building and Running

**Prerequisites:**
*   Node.js 18+
*   Rust 1.70+
*   Proxmark3 client binary (placed in `src-tauri/binaries/`)

**Commands:**

*   **Install Dependencies:**
    ```bash
    npm install
    ```

*   **Run in Development Mode:**
    ```bash
    npm run dev
    # OR
    npx tauri dev
    ```
    This starts the Vite dev server and the Tauri application window.

*   **Build for Production:**
    ```bash
    npm run build
    # OR
    npx tauri build
    ```
    This compiles the React app and the Rust backend, producing an executable (NSIS installer on Windows, DMG/App bundle on macOS).

*   **Preview Build:**
    ```bash
    npm run preview
    ```

## Development Conventions

**Directory Structure:**

*   `src/`: Frontend source code.
    *   `components/`: React components organized by feature (e.g., `wizard`, `history`, `settings`).
    *   `hooks/`: Custom React hooks (`useWizard`, `useSettings`, etc.).
    *   `machines/`: XState machine definitions (`wizardMachine.ts`).
    *   `lib/`: Utility functions and constants.
    *   `styles/`: Global styles and Tailwind configuration.
*   `src-tauri/`: Backend source code.
    *   `src/`: Rust source files.
        *   `commands/`: Tauri command handlers, organized by domain (e.g., `device`, `scan`, `write`).
        *   `pm3/`: Logic for interacting with the Proxmark3 client.
        *   `db/`: Database models and operations.
        *   `state.rs`: Backend state machine logic.
    *   `binaries/`: Directory for the external Proxmark3 executable.
    *   `tauri.conf.json`: Main Tauri configuration file.

**Coding Style:**
*   **Frontend:** Functional React components with hooks. TypeScript for type safety. Tailwind CSS for styling.
*   **Backend:** Idiomatic Rust. Commands are exposed via `#[tauri::command]`. Error handling uses `thiserror`.
*   **Logging:** The backend uses `log` and `env_logger`.

**Key Technologies:**
*   **Tauri:** v2 (Application framework)
*   **React:** v19 (UI library)
*   **Vite:** v7 (Build tool)
*   **XState:** v5 (State management)
*   **Rusqlite:** (SQLite database)
