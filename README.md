<div align="center">
  <img src="assets/filedna.svg" alt="FileDNA logo" width="112" />

  # FileDNA

  **Inspect files. Compare contents. Find duplicates.**

  Alpha release · Rust · egui
</div>

[English](#english) · [Deutsch](#deutsch)

## English

FileDNA is a desktop application for inspecting files. It displays file metadata and calculates SHA-256 fingerprints to identify files with identical contents.

### Screenshot

![FileDNA Alpha interface](screenshots/filedna-alpha.png)

### Features

- Analyze multiple files and folders together
- Optionally include subfolders recursively
- View file name, path, size, modification time, and SHA-256 hash
- Group duplicate files by SHA-256 hash
- Compare two selected files
- Export results as JSON or CSV, or copy them to the clipboard
- Show progress and cancel scans or comparisons
- Hash files in chunks without loading the entire file into memory
- Window icon and close button

### Getting started

Requirements: Rust and Cargo (Edition 2024), and a Linux desktop environment.

Run from the project directory:

```bash
cargo run
```

You can pass a file or folder as an input. FileDNA starts analyzing it when the window opens:

```bash
cargo run -- ./beispiel.txt
```

Build an optimized release:

```bash
cargo build --release
```

### How to use it

1. Add inputs with **Dateien hinzufügen…** or **Ordner hinzufügen…**.
2. Enable **Unterordner einbeziehen** if needed.
3. Click **Analyse starten**. Click **Abbrechen** to stop a running scan.
4. Results appear in the table; matching hashes are grouped under **Duplikate**.
5. Select two files and click **Vergleichen** to compare them.
6. Use the export buttons to save or copy the results.
7. Click **Schließen** to exit the application.

Symbolic links are not followed during folder scans. Unreadable files and folders are listed as errors or notices.

### Project status

This is an early alpha release. The interface and features may change. Feedback and bug reports are welcome.

### License

FileDNA is licensed under the MIT License. See [LICENSE](LICENSE) for the full license text.

## Deutsch

FileDNA ist ein Desktop-Programm zur Analyse von Dateien. Es zeigt Dateimetadaten an und berechnet SHA-256-Fingerabdrücke, um Dateien mit identischem Inhalt zu erkennen.

### Screenshot

![FileDNA Alpha-Oberfläche](screenshots/filedna-alpha.png)

### Funktionen

- Mehrere Dateien und Ordner gemeinsam untersuchen
- Unterordner optional rekursiv einbeziehen
- Dateiname, Pfad, Größe, Änderungsdatum und SHA-256 anzeigen
- Duplikate anhand ihres SHA-256-Hashes gruppieren
- Zwei ausgewählte Dateien vergleichen
- Ergebnisse als JSON oder CSV exportieren oder in die Zwischenablage kopieren
- Fortschritt anzeigen und Analysen oder Vergleiche abbrechen
- Dateien blockweise hashen, ohne den gesamten Inhalt in den Arbeitsspeicher zu laden
- Fenster-Icon und Schließen-Button

### Starten

Voraussetzungen: Rust und Cargo (Edition 2024) sowie eine Linux-Desktop-Umgebung.

Im Projektverzeichnis:

```bash
cargo run
```

Eine Datei oder einen Ordner kannst du direkt als Eingabe übergeben. FileDNA startet die Analyse beim Öffnen:

```bash
cargo run -- ./beispiel.txt
```

Einen optimierten Release-Build erstellst du so:

```bash
cargo build --release
```

### Bedienung

1. Über **Dateien hinzufügen…** oder **Ordner hinzufügen…** Eingaben auswählen.
2. Bei Bedarf **Unterordner einbeziehen** aktivieren.
3. **Analyse starten** wählen. Mit **Abbrechen** lässt sich eine laufende Analyse stoppen.
4. Die Ergebnisse erscheinen in der Tabelle; gleiche Hashes werden unter **Duplikate** gruppiert.
5. Zwei Dateien auswählen und auf **Vergleichen** klicken, um sie zu prüfen.
6. Über die Export-Schaltflächen Ergebnisse speichern oder kopieren.
7. Mit **Schließen** das Programm beenden.

Symbolische Links werden bei Ordneranalysen nicht verfolgt. Nicht lesbare Dateien und Ordner erscheinen als Fehler oder Hinweise.

### Projektstatus

Dies ist eine frühe Alpha-Version. Oberfläche und Funktionen können sich noch ändern. Rückmeldungen und Fehlerberichte sind willkommen.

### Lizenz

FileDNA steht unter der MIT-Lizenz. Den vollständigen Lizenztext findest du in [LICENSE](LICENSE).
