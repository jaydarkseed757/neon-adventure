use crate::ui;

// ---------------------------------------------------------------------------
// Minimal xorshift64 RNG — no external crates needed
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xcafe_beef_dead_1234);
        Rng(if seed == 0 { 1 } else { seed })
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn range(&mut self, n: usize) -> usize {
        if n == 0 { return 0; }
        (self.next() as usize) % n
    }

    fn one_in(&mut self, n: u64) -> bool {
        if n == 0 { return false; }
        self.next() % n == 0
    }
}

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

pub struct Ambient {
    rng: Rng,
}

impl Ambient {
    pub fn new() -> Self {
        Ambient { rng: Rng::new() }
    }

    /// Called after each command. Prints a random atmospheric line ~1-in-4.
    pub fn tick(&mut self, room_id: &str, turn: u32) {
        if !self.rng.one_in(4) {
            return;
        }
        let pool = ambient_pool(room_id, time_period(turn));
        if pool.is_empty() {
            return;
        }
        let msg = pool[self.rng.range(pool.len())];
        ui::print_blank();
        ui::print_ambient(msg);
    }
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// Time period 0-7.
/// Full day cycle = 120 turns. Game starts at late afternoon (offset 60).
///   0 Dawn  1 Morning  2 Midday  3 Afternoon
///   4 Late Afternoon  5 Dusk  6 Evening  7 Night
pub fn time_period(turn: u32) -> u32 {
    ((turn as u64 + 60) % 120 / 15) as u32
}

pub fn time_label(turn: u32) -> &'static str {
    match time_period(turn) {
        0 => "Dawn",
        1 => "Morning",
        2 => "Midday",
        3 => "Afternoon",
        4 => "Late Afternoon",
        5 => "Dusk",
        6 => "Evening",
        _ => "Night",
    }
}

// ---------------------------------------------------------------------------
// Time flavor lines — shown once when looking at a room
// ---------------------------------------------------------------------------

const OUTDOOR: &[&str] = &[
    "front_porch", "gravel_path", "dead_garden", "garden_gate",
    "woods_edge", "kitchen_garden", "groundskeepers_shed",
    "stable", "gatehouse", "deep_woods", "greenhouse",
];

const WINDOWED: &[&str] = &[
    "foyer", "library", "ballroom", "dining_room", "coat_room", "chapel",
    "master_bedroom", "childs_room", "guest_room", "smoking_room",
    "upper_landing", "attic", "tower_room", "washroom", "servants_quarters",
    "study",
];

/// Short atmospheric line based on time of day, printed when the player looks at a room.
/// Returns None for underground/windowless rooms where time is irrelevant.
pub fn time_flavor(room_id: &str, turn: u32) -> Option<&'static str> {
    let p = time_period(turn);

    if OUTDOOR.contains(&room_id) {
        return Some(match p {
            0 => "The urban fog is at maximum density. Shapes emerge from it slowly.",
            1 => "Thin morning light filters down through the overcast sky.",
            2 => "Grey ambient light sits flat and cold over everything.",
            3 => "The light is beginning to thin. Shadows extend further.",
            4 => "The sun is clearly below the tower line. The cold deepens.",
            5 => "The last of the usable daylight bleeds from the sky.",
            6 => "Full dark has settled. The fog swallows what little there is to see.",
            _ => "The night is complete. The dark presses in from every signal direction.",
        });
    }

    if WINDOWED.contains(&room_id) {
        return Some(match p {
            0 => "Thin predawn light seeps through the composite panels.",
            1 => "Pale morning light filters through the exterior glazing.",
            2 => "Flat grey ambient light fills the room.",
            3 => "The afternoon light is thinning visibly through the glazing.",
            4 => "Amber light slants through the panels, throwing long shadows across the composite floor.",
            5 => "The last usable light drains from the exterior panels.",
            6 => "The panels hold nothing now but their own dead reflections.",
            _ => "The exterior panels are completely dark. Whatever is outside is not registering.",
        });
    }

    None
}

// ---------------------------------------------------------------------------
// Ambient event pools
// ---------------------------------------------------------------------------

fn ambient_pool(room_id: &str, period: u32) -> &'static [&'static str] {
    // Room-specific events take priority over category pools
    match room_id {
        "foyer" => return &[
            "The ventilation system cycles briefly. The upload terminal's amber light pulses once.",
            "Somewhere deeper in the building, an access door opens on its automated hinge. Somewhere, it closes.",
            "The cold air intake exhales a breath of processed atmosphere.",
            "A display panel somewhere in the building activates briefly. By the time you locate it, it has gone dark.",
            "The composite on the upload platform carries the faint impression of things that once rested on it.",
        ],
        "library" => return &[
            "A server unit in the far row cycles through a diagnostic state and returns to standby.",
            "The maintenance ladder vibrates with a low resonant frequency.",
            "Indicator lights shift across a server stack in a pattern you cannot interpret.",
            "The workstation holds very still. Too still, given what is running inside it.",
        ],
        "childs_room" => return &[
            "The ergonomic chair shifts, almost imperceptibly, on its base.",
            "The audio player on the shelf produces a single tick. It is not wound.",
            "The drawings on the wall seem to extend further along the surface than you remembered.",
            "The ambient temperature in this unit is higher than the rest of the floor. You don't have a calibrated explanation.",
        ],
        "crypt" => return &[
            "The silence in here is not the silence of an inactive room.",
            "Processed air moves through the chamber from no visible intake.",
            "A carrier frequency — just below detection threshold — like a very slow transmission.",
            "One of the node clusters casts an indicator shadow that doesn't correspond to its current state.",
        ],
        "vault" => return &[
            "The broken lock housings shift slightly in the airflow from the Protocol Core.",
            "The empty storage niches register like open access points.",
            "Something cycles very slowly through the ceiling infrastructure. The floor beneath is dry.",
        ],
        "master_bedroom" => return &[
            "The fractured display surface catches a reflection. It is only you.",
            "The environmental regulation fixtures shift in a micro-adjustment from no logged cause.",
            "The heating panel has acquired a slightly different output level since you last checked.",
            "The sleeping platform hasn't registered occupancy in years. The thermal impression says otherwise.",
        ],
        "tower_room" => return &[
            "The signal array trembles slightly on its precision mount.",
            "The orbital maps cast unusual shadows when the ambient light shifts.",
            "Something transmits in the signal environment beyond the hardened panel. A satellite, probably.",
            "The last observation in the log was three years ago. Someone has added a question mark after the final entry.",
        ],
        "attic" => return &[
            "Something repositions in the server clusters above. Thermal expansion, probably.",
            "A storage case settles with a long resonant sound, as if something internal shifted.",
            "A fragment of composite falls from a junction overhead. Something moved up there.",
            "The skylight brightens slightly, then normalizes. Cloud cover over the city.",
        ],
        "deep_woods" => return &[
            "A structural element shifts somewhere in the sprawl. Nothing approaches.",
            "The urban fog takes on shapes and releases them.",
            "Signal coverage drops suddenly to zero, as if something is suppressing it.",
            "Wind moves through the infrastructure above without reaching you.",
        ],
        "crypt_entrance" => return &[
            "The text above the archway seems to have a higher rendering priority than the composite around it.",
            "Processed air moves steadily out of the passage ahead.",
            "The dead light source at the entry still carries a faint thermal signature.",
        ],
        "groundskeepers_shed" => return &[
            "The heating element in the corner ticks as it cycles. Or something moves within it.",
            "The coiled cable on the workbench shifts position without an applied force.",
            "The access door registers a contact event once, then returns to null. No external wind.",
        ],
        _ => {}
    }

    // Category fallbacks
    let underground = matches!(room_id,
        "wine_cellar" | "root_cellar" | "tunnel" | "cellar_stairs"
    );
    if underground {
        return &[
            "Water cycles somewhere in the deep infrastructure. The sound carries farther than it should.",
            "The cold here is not the cold of temperature regulation. It settles into the composite differently.",
            "A faint smell of something that has been running for longer than this building reaches you.",
            "You run a listen operation. The silence down here is not a default state.",
            "The walls are denser than they were a moment ago. They are not.",
            "Something shifts in the deep infrastructure. Thermal regulation. Probably thermal regulation.",
        ];
    }

    let outdoor = OUTDOOR.contains(&room_id);
    if outdoor {
        return match period {
            0..=1 => &[
                "The urban fog is thick enough to suppress ambient signal.",
                "Something moves in the fog. It does not approach.",
                "The composite underfoot registers differently, as if the substrate is unresolved.",
                "The arcology behind you is barely visible in the fog. Every access point dark.",
            ],
            5..=7 => &[
                "In the failing light, the arcology's panels are blank and dark.",
                "The fog thickens as the ambient level drops.",
                "Something in the distance — a signal, perhaps — registers once and drops.",
                "The night-level signals begin. You'd prefer they didn't.",
                "The cold has real operational weight now.",
            ],
            _ => &[
                "The fog shifts, and for a moment you register something in it.",
                "Wind moves the abandoned infrastructure. Nothing else registers.",
                "The arcology is behind you. Every access point is dark.",
                "Something in the sprawl. It doesn't register as a standard signal source.",
                "A light rain begins, then stops. The silence after is a different silence.",
            ],
        };
    }

    let upper = matches!(room_id, "upper_landing" | "guest_room" | "smoking_room");
    if upper {
        return &[
            "A floor panel settles somewhere down the corridor.",
            "Behind one of the closed access doors, a signal. You hold your query state. It does not repeat.",
            "Air circulation moves along the executive level from no active intake.",
            "The portrait panels seem to track the position you just vacated.",
            "Somewhere above, a structural element registers under roof load.",
        ];
    }

    let service = matches!(room_id,
        "servants_hall" | "kitchen" | "washroom" | "servants_quarters" | "pantry"
    );
    if service {
        return &[
            "A status indicator pulses once in the access panel row. None of them should still be active.",
            "The pipes inside the wall infrastructure cycle and go silent.",
            "From above: footsteps. One, two, three. Then nothing.",
            "A thermal signature — almost the profile of something being processed — registers and is gone.",
        ];
    }

    // Generic interior fallback
    &[
        "Somewhere in the arcology, an access point registers. Somewhere in the arcology, it closes.",
        "The cold is consistent, sourceless, and absolute.",
        "Something shifts at the edge of your visual field. Nothing is there.",
        "The recycled air carries something for a moment, then doesn't.",
        "The arcology processes around you with a sound like a distributed system making small adjustments.",
    ]
}
