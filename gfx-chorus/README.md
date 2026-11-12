# gfx-chorus

> **Statut : PROPOSÉ** — conception, pas de code.

Un chorus complet, voicé par instrument.

## Façade (bloc golem)

Profil · Mode · Rate · Depth · Warmth · Mix. Tout le reste est en mode expert, avec des défauts posés
par le profil.

## Profils

Guitare clean · Guitare saturée · Basse · Synthé/claviers · Voix. Un profil fixe les plages, les filtres,
le crossover et la quantité de couleur.

## Chaîne

1. Profil d'instrument
2. Crossover : graves sèches et mono sous une fréquence réglable
3. Étage d'entrée : saturation douce, pré-accentuation
4. Moteur de retard : Clean (interpolation haute qualité) · BBD (filtres AA/reconstruction, compandeur,
   bruit d'horloge) · Tape (wow/flutter)
5. Topologie : Classic · Dual (Juno I/II) · Dimension (voix complémentaires, cross-mix) · Ensemble (3–6
   voix) · Doubler (dérive aléatoire)
6. LFO : sinus, triangle arrondi, aléatoire lissé, triangle Juno ; dérive, phase stéréo, synchro tempo
7. Filtrage du wet : passe-haut, passe-bas (chaleur)
8. Stéréo : largeur, mono → stéréo, mode compatible mono
9. Sortie : mix, compensation de niveau

## Contraintes golem

Zéro latence (pas d'oversampling à phase linéaire), `check-block-stall` vert sur les deux moteurs,
identifiants de paramètres stables, lissage compatible pédale EXP et changements de scène sans clic.
