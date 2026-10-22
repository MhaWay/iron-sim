//! 5-locale string table (en/it/es/de/fr) — same simple key table pattern as rts game-ui,
//! extended to 5 languages per task spec. No external assets, pure static strings.

#[derive(Clone, Copy, Debug)]
pub struct L(pub &'static str, pub &'static str, pub &'static str, pub &'static str, pub &'static str);
impl L {
    /// Translate by locale code; unknown falls back to English.
    pub fn t(self, locale: &str) -> &'static str {
        match locale {
            "it" => self.1,
            "es" => self.2,
            "de" => self.3,
            "fr" => self.4,
            _ => self.0,
        }
    }
}

pub const LOCALES: [&str; 5] = ["en", "it", "es", "de", "fr"];
pub const LOCALE_NAMES: [(&str, &str); 5] = [
    ("en", "English"),
    ("it", "Italiano"),
    ("es", "Español"),
    ("de", "Deutsch"),
    ("fr", "Français"),
];

pub const MENU_TITLE: L = L("Iron-Sim Colony", "Iron-Sim Colonia", "Iron-Sim Colonia", "Iron-Sim Kolonie", "Iron-Sim Colonie");
pub const START_NEW: L = L("Start new colony", "Nuova colonia", "Nueva colonia", "Neue Kolonie", "Nouvelle colonie");
pub const CONTINUE: L = L("Continue", "Continua", "Continuar", "Fortsetzen", "Continuer");
pub const SANDBOX_MODE: L = L("Sandbox (no raiders / no enemies)", "Sandbox (nessun nemico)", "Modo libre (sin enemigos)", "Sandbox (keine Feinde)", "Sandbox (sans ennemis)");
pub const CAMPAIGN_MODE: L = L("Campaign (raids, disease)", "Campagna (raid, malattie)", "Campaña (incursiones, enfermedad)", "Kampagne (Überfälle, Krankheit)", "Campagne (raids, maladie)");
pub const DIFFICULTY: L = L("Difficulty", "Difficoltà", "Dificultad", "Schwierigkeit", "Difficulté");
pub const SEED: L = L("Seed", "Seme", "Semilla", "Startwert", "Graine");
pub const RANDOM: L = L("Random", "Casuale", "Aleatorio", "Zufall", "Aléatoire");
pub const PRESET: L = L("Preset", "Preimpostato", "Preajuste", "Voreinstellung", "Préréglage");
pub const PLAY_BTN: L = L("PLAY", "AVVIA", "JUGAR", "START", "JOUER");
pub const BACK: L = L("< Back", "< Indietro", "< Volver", "< Zurück", "< Retour");
pub const EXIT: L = L("Exit", "Esci", "Salir", "Exiten", "Quitter");
pub const LANGUAGE: L = L("Language", "Lingua", "Idioma", "Sprache", "Langue");
pub const INVALID_CFG: L = L("Invalid configuration", "Configurazione non valida", "Configuración invalida", "Ungültige Konfiguration", "Configuration invalide");
pub const NEW_COLONY: L = L("New colony", "Nuova colonia", "Nueva colonia", "Neue Kolonie", "Nouvelle colonie");
pub const PAUSED: L = L("PAUSED", "PAUSA", "PAUSADO", "PAUSSPIELE", "ARRÊT");
pub const PLAY_SPEED: L = L("▶ Play", "▶ Riprendi", "▶ Seguir", "▶ Fortfahren", "▶ Reprendre");
pub const PAUSE_BTN: L = L("⏸ Pause", "⏸ Pausa", "⏸ Pausa", "⏸ Pause", "⏸ Pause");
pub const SAVE_GAME: L = L("Save game", "Salva partita", "Guardar partida", "Spiel speichern", "Sauvegarder");
pub const LOAD_GAME: L = L("Load game", "Carica partita", "Cargar partida", "Spiel laden", "Charger");
pub const BACK_MENU: L = L("Menu", "Menu", "Menu", "Menü", "Menu");
pub const COLONY_PANEL: L = L("Colony", "Colonia", "Colonia", "Kolonie", "Colonie");
pub const JOB_QUEUE: L = L("Job queue", "Coda lavori", "Cola de trabajo", "Auftragsliste", "File de tâches");
pub const PRIORITIES: L = L("Priorities", "Priorità", "Prioridades", "Prioritäten", "Priorités");
pub const PAWNS_PANEL: L = L("Pawns", "Coloni", "Colonos", "Ansassen", "Colons");
pub const HUNGER: L = L("Hunger", "Fame", "Hambre", "Hunger", "Faim");
pub const ENERGY: L = L("Energy", "Energia", "Energía", "Energie", "Énergie");
pub const MOOD: L = L("Mood", "Umore", "Ánimo", "Laune", "Humeur");
pub const HEALTH: L = L("Health", "Salute", "Salud", "Gesundheit", "Santé");
pub const ASSIGN: L = L("Assign", "Assegna", "Asignar", "Zuweisen", "Assigner");
pub const BUILDINGS: L = L("Build", "Costruisci", "Construir", "Bauen", "Construire");
pub const STOR_SHELF: L = L("Storage shelf", "Scaffale", "Estantería", "Regal", "Étagère");
pub const BED_BUILD: L = L("Bed", "Letto", "Cama", "Bett", "Lit");
pub const FARM_BUILD: L = L("Farm plot", "Podere", "Parcela", "Feld", "Parcelle");
pub const WORK_ZONE: L = L("Work zone", "Zona di lavoro", "Zona de trabajo", "Arbeitsbereich", "Zone de travail");
pub const CLICK_MAP: L = L("Click the map (right-click: place / rotate / cancel)", "Clicca la mappa (destro-click: piazza / ruota / annulla)", "Clic el mapa (clic derecho: colocar / girar / cancelar)", "Klick die Karte (Rechtsklick: platzieren / rotieren / entfällt)", "Clic sur la carte (clic droit : placer / tourner / annuler)");
pub const CLICK_MAP_SIMPLE: L = L("Click the map", "Clicca la mappa", "Clic el mapa", "Klick die Karte", "Clic sur la carte");
pub const SELECTED: L = L("Selected", "Selezionato", "Seleccionado", "Ausgewählt", "Sélectionné");
pub const CLICK_PAWN: L = L("Click a pawn to select; right-click: move / build", "Clicca un pawn per selezionare, destro-click per muovere/costruire", "Clic un peón para seleccionar, click derecho para mover/construir", "Klick einen Pawn zum Auswählen, Rechtsklick zum Bewegen/Bauen", "Clic sur un pion pour sélectionner, clic droit pour bouger/bâtir");
pub const TICK: L = L("Tick", "Tick", "Tick", "Tropfen", "Tour");
pub const POP: L = L("Pop", "Pop", "Pob", "Bev", "Pop");
pub const MODE_L: L = L("Mode", "Modo", "Modo", "Modus", "Mode");
pub const IDLE: L = L("Idle", "Niente", "Desocupado", "Leer", "Inactif");
pub const WALKING: L = L("Walking", "Cammina", "Caminando", "Laufen", "Marche");
pub const EATING: L = L("Eating", "Mangia", "Comiendo", "Essen", "Mange");
pub const SLEEPING: L = L("Sleeping", "Dorme", "Durmiendo", "Schlafen", "Dort");
pub const WORKING: L = L("Working", "Lavora", "Trabajando", "Arbeitet", "Travaille");
pub const ALERT_STARVE: L = L("PAWN STARVING", "PAGN IN FAME", "PEÓN PASANDO HAMBRE", "PAUN HUNDERNT", "PON À LA MOURIR DE FAIM");
pub const ALERT_EXHAUST: L = L("PAWNS EXHAUSTED", "PAGN ESAUSTI", "PEONES AGOTADOS", "PAUNE ERSCHÖPFT", "PONS ÉPUISÉS");
pub const ALERT_DISEASE: L = L("DISEASE OUTBREAK", "SCOPPIO DI MALATTIA", "BROTE DE ENFERMEDAD", "KRANKHEITS-AUSBRRUCH", "ÉCLOSÉE DE MALADIE");
pub const ALERT_RAID: L = L("RAIDS INCOMING", "INCURSIONI IN ARRIVO", "INCURSIONES ENTRANTESS", "RAIDS IM ANMARSCH", "RAIDS EN ATTENTE");
pub const NO_JOBS: L = L("No open jobs", "Nessun lavoro aperto", "Sin trabajos abiertos", "Keine offenen Aufträge", "Aucune tâche ouverte");
pub const SAVE_OK: L = L("Saved.", "Salvato.", "Guardado.", "Gespeichert.", "Enregistré.");
pub const LOAD_OK: L = L("Loaded.", "Caricato.", "Cargado.", "Geladen.", "Chargé.");
pub const SAVE_FAIL: L = L("Save/load failed", "Salvataggio/caricamento fallito", "Guardado/carga fallida", "Speichern/Laden fehlgeschlagen", "Échec de sauvegarde/chargement");
pub const NO_SAVE: L = L("No save on disk.", "Nessun salvataggio.", "Sin guardado.", "Kein Save.", "Aucune sauvegarde.");
pub const SPEED: L = L("Speed", "Velocità", "Velocidad", "Tempo", "Vitesse");

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locale_lookup_falls_back_to_english() {
        assert_eq!(EXIT.t("en"), "Exit");
        assert_eq!(EXIT.t("it"), "Esci");
        assert_eq!(EXIT.t("es"), "Salir");
        assert_eq!(EXIT.t("de"), "Exiten");
        assert_eq!(EXIT.t("fr"), "Quitter");
        assert_eq!(EXIT.t("xx"), "Exit");
        assert_eq!(LOCALES.len(), 5);
    }
}
