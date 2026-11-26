# Roadmap 0.8

Décidée le 2026-11-26. Point de départ : la release 0.7 (l'éditeur gfx-ui sur les 56 pédales) et la comparaison de la
gamme avec la liste de Rob Chapman (Guitar Pedal X, 2023-12-04). Les noms de marque (MXR, EHX, Boss, Nordland,
Marshall) servent de références de son ; ils n'apparaissent ni dans le nom des pédales ni dans leurs paramètres
(comme Dist et son « Blend OD » pour un Klon).

Ordre : 1 → 2 → 3 → 4. Les lignes « Fait quand » sont les critères pour cocher.

## 1. Fuzz octave — référence MXR Octavio (Roger Mayer Octavia)

Nouvelle pédale, `gfx-octafuzz` (nom de travail ; nom affiché « GoleM FX Octafuzz »).

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

- Ajouter : `trigger` (booléen, momentané ou verrouillé), `rise` (temps de montée), `fall` (temps de retour). Hors
  déclenchement, Pitch reste exactement ce qu'il est (ids `shift` et `mix` du bloc `pitch` inchangés).
- Le trigger se branche sur un footswitch de golem.
- Fait quand : montée et retour suivent les temps réglés (test), aucun changement pour un preset existant.

## Plus tard (idées notées, non décidées)

Guitare : filtre synthé à LFO / séquenceur / sample & hold ; talk box à l'expression ; séquenceur de hauteur.
Basse : simulateur de contrebasse / fretless ; excitateur de graves psychoacoustique ; drop-tune à faible latence ;
séparateur grave / aigu devant n'importe quel effet ; EQ dynamique.
