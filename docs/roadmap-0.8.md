# Roadmap 0.8

Décidée le 2026-11-26. Point de départ : la release 0.7 (l'éditeur gfx-ui sur les 56 pédales) et la comparaison de la
gamme avec la liste de Rob Chapman (Guitar Pedal X, 2023-12-04). Les noms de marque (MXR, EHX, Boss, Nordland,
Marshall) servent de références de son ; ils n'apparaissent ni dans le nom des pédales ni dans leurs paramètres
(comme Dist et son « Blend OD » pour un Klon).

Ordre : 1 → 2 → 3 → 4. Les lignes « Fait quand » sont les critères pour cocher.

## État — conception complète et relecture Fable 5.2 (2026-11-26)

Les quatre points sont conçus (Claude Opus 5.5) et relus (Claude Fable 5.2) ; le détail, les chiffres et les
questions ouvertes sont dans chaque README (section « Review (Fable 5.2, 2026-11-26) » à la fin). Rien n'est codé.
Deux points de cette feuille de route étaient déjà en partie conçus avant 0.8, ce qui corrige les sections plus bas :

| Point | Où | Verdict | Ce qui bloque le codage |
|---|---|---|---|
| 1. Fuzz octave | `gfx-fuzz/README.md`, modèle **Octave Up** (déjà prévu, jamais codé), désormais le type à **transformateur** de l'Octavio ; netlist d'après madbean Retrograde24, Aion Octagon, PCB Guitar Mania Octavius | Bon | CPU ≈ 2× le contrat (30–50 % d'un cœur) ; inductance magnétisante du transformateur 42TM022 jamais publiée (à mesurer) ; pédale à part (`gfx-octafuzz` sur le moteur de gfx-fuzz, conseil Fable) ou modèle de Fuzz |
| 2. Orgue | `gfx-organ/README.md` (nouvelle pédale) sur le cœur Spectral de Polysynth ; tirettes = puissances de la phase de chaque bande, sans suivi de hauteur ; Leslie intégré | Vaut d'être construit, **à risque** sur un point | Le registre complet arrive 100–170 ms après l'attaque (le 8' à 20 ms) ; le « tamis des fondamentales » doit être prouvé sur des DI annotées avant tout autre code ; déplacement du cœur Spectral, du rotor et du scanner dans gfx-dsp (nulls bit à bit + CPU à 3 %) |
| 3. Trois voicings Dist | `gfx-dist/README.md` : **Discrete OD** (le Blues Driver, déjà conçu, jamais codé ; + `fat` pour la variante Custom ; nouveau modèle de paire JFET dans circuit_nl), **Studio OD** (ODR-1), **Plexi Dist** (Red Rox, + `contour`) | Solide · Solide · Bon | CPU ≈ 24 % d'un cœur pour Discrete OD et Plexi (à mesurer) ; alias du Plexi à 18 V ; l'enum `circuit` passe de 4 à 7 : catalogue de golem et VST3 dans la même release |
| 4. Bend déclenché de Pitch | `gfx-pitch/README.md` : `bend` ajouté à `shift` pendant le déclenchement, `rise` / `fall` séparés, courbes Linear · Smooth, + `rest` (Mix · Dry) | Solide, codable | Nom de l'id du geste (`trigger` existe déjà ailleurs) ; le footswitch de golem n'envoie pas encore le relâchement (momentané) |

### Décisions de Raph (2026-11-26 : les recommandations des relecteurs, acceptées)

1. Fuzz octave : **pédale à part, `gfx-octafuzz`**, mince, sur le moteur de gfx-fuzz ; Fuzz et sa façade ne changent
   pas, `octave` est sur la façade de gfx-octafuzz.
2. Son CPU : la netlist est d'abord codée dans gfx-bench et mesurée ; une **exception CPU pour cette pédale seule**
   est acceptée si la mesure reste au-dessus du contrat (30–40 % attendus).
3. Transformateur : **mesurer un 42TM022** (inductance selon le niveau, résistances, fuite, saturation) — Raph.
4. Orgue : le registre complet à 100–170 ms (le 8' à 20 ms) est accepté pour l'instant, à juger à l'oreille.
5. Orgue : Raph enregistre des **DI annotées** (accords, arpèges, lignes rapides, notes connues) ; prouver le tamis
   des fondamentales dessus est la première tâche de code de l'orgue.
6. Plexi Dist : `supply` plafonné à **150 %** ; le preset « 18 Volt Plexi » est abandonné ; `contour` compte comme un
   réglage pour la règle d'alias.
7. Discrete OD et Plexi Dist : ≈ 24 % d'un cœur accepté si la mesure codée donne **≤ 5 µs par échantillon**.
8. Pitch : le geste s'appelle **`bend`** (booléen) et l'intervalle **`interval`**.
9. Pitch : **`rest`** (Mix · Dry) accepté ; défauts 100 / 250 ms, 8 s au plus, Linear.

Le reste suit les recommandations des relecteurs (noms des circuits, `fat` en interrupteur, `contour` en expert,
`mod` de l'orgue en profondeur, crans de tirettes de 3 dB à confirmer à l'oreille, libellé « Rotor »…).

Trouvé en passant (à corriger dans 0.8) : **Pitch v0.1.0, après un bend, ne revient pas au son de départ** — le wet
ne s'annule plus contre le dry, et le mode Poly reste 2–3 dB plus bas (re-centrer les têtes et remettre les phases du
vocodeur à ratio 1, gfx-dsp).

## 1. Fuzz octave — référence MXR Octavio (Roger Mayer Octavia)

Nouvelle pédale, `gfx-octafuzz` (« GoleM FX Octafuzz »), mince, sur le moteur de gfx-fuzz ; le circuit (modèle
Octave Up, à transformateur) est conçu dans `gfx-fuzz/README.md`.

- Le son : une fuzz dont l'octave supérieure naît d'un redressement double alternance (transformateur et diodes
  dans l'original), qui chante surtout sur les notes aiguës, près des micros manche, tonalité en retrait.
- Le circuit simulé comme Fuzz et Dist le font déjà : étage de fuzz, déphaseur, redresseur, étage de sortie.
  Repartir de gfx-dsp (les modèles de Fuzz : Ge Pair, Si Four) plutôt que d'un redressement idéal.
- Réglages de base : Level, Fuzz. En plus : un mélange de l'octave (tout octave ↔ fuzz seule), Tone, Bass Keep
  (comme les autres pédales de saturation, pour la basse).
- Pédale complète : crate, `bundler.toml`, dessin dans `visuals/js/golem-pedals.js`, paquet gfx-ui (`build-ui.js`,
  `meters.json` : rien à afficher, ou une lampe), presets d'usine, branchement dans le catalogue de golem.
- Fait quand : l'octave est mesurable au banc (gfx-bench : 2f nettement au-dessus de f sur une note aiguë),
  aliasing sous le seuil des autres fuzz, latence 0, le contrôle de découpage de build-ui passe.

## 2. Orgue — référence EHX B9 Organ Machine

Nouvelle pédale, `gfx-organ` (nom de travail ; « GoleM FX Organ »).

- Le son : la guitare (polyphonique) devient un orgue : tirettes, percussion, clic de touche, Leslie.
- Base existante : le moteur polyphonique de **Polysynth** (suivi des notes) et **Rotary** (le Leslie, à réutiliser
  dans la pédale ou à laisser derrière elle — à décider).
- Neuf registres prédéfinis, à la manière du B9 : Fat & Full, Jazz, Gospel, Classic Rock, Cathedral, Bell, Mellow,
  Continental, Octave Organ (noms à nous, pas ceux d'EHX). Réglages : Organ (niveau), Dry, Modulation
  (vitesse du Leslie / vibrato), Click.
- Expert : les tirettes elles-mêmes (16', 8', 5⅓', 4'…), la percussion (2e / 3e harmonique, decay).
- Fait quand : les accords suivent sans note fantôme, latence au plus celle de Polysynth, CPU dans le budget de golem.

## 3. Trois voicings d'overdrive dans Dist

Nouveaux modèles de **Dist** (pas de nouvelle pédale), à côté de Op-Amp Dist, Booster Dist, Blend OD, MOSFET OD.

- **Blues Driver** (Boss BD-2) : étages discrets, médium plat, grain qui se nettoie au volume de la guitare. Prévoir
  la variante « Custom » (Keeley Phat Mod : plus de grave).
- **ODR-1** (Nordland ODR-1, Vemuram Shanks) : la « natural overdrive », bosse dans le haut-médium et son réglage
  Spectrum.
- **Plexi** (Marshall-in-a-box : Dawner Prince Red Rox, Rockbox Boiling Point) : un Marshall 1959 en pédale, pour un
  ampli clair. Réglages propres : Contour / Preamp.
- Pour chacun : simulation du circuit comme les modèles actuels, presets d'usine, façade de Dist mise à jour si le
  sélecteur de modèle change (paquet gfx-ui régénéré).
- Fait quand : chaque modèle a sa courbe de réponse mesurée au banc, et les anciens presets de Dist sonnent pareil.

## 4. Pitch : le mouvement de la Whammy Ricochet

Vérifié le 2026-11-26 : Pitch (moteur Whammy) n'a qu'un **Glide** (0–1000 ms, le même à la montée et au retour) et
pas de déclencheur ; on le joue en pilotant `shift` à la pédale d'expression. La Ricochet, elle, monte vers
l'intervalle à l'appui et revient au relâchement, avec deux temps séparés.

- Ajouter : `bend` (booléen, le geste), `interval`, `rise` (temps de montée), `fall` (temps de retour), `curve`, `rest`. Hors
  déclenchement, Pitch reste exactement ce qu'il est (ids `shift` et `mix` du bloc `pitch` inchangés).
- Le trigger se branche sur un footswitch de golem.
- Fait quand : montée et retour suivent les temps réglés (test), aucun changement pour un preset existant.

## Plus tard (idées notées, non décidées)

Guitare : filtre synthé à LFO / séquenceur / sample & hold ; talk box à l'expression ; séquenceur de hauteur.
Basse : simulateur de contrebasse / fretless ; excitateur de graves psychoacoustique ; drop-tune à faible latence ;
séparateur grave / aigu devant n'importe quel effet ; EQ dynamique.
