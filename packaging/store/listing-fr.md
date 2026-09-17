# Store listing — French (fr-FR)

The French listing for the Microsoft Store submission (#126), the translation of
`listing-en.md`. It uses the words the French interface uses: *piloter*,
*appareil*, *effets*, *paramètres*.

Version described: **0.7.0**. Its *What's new* covers everything since 0.5.1, the
version the Store still carries: 0.6.0 was never submitted.

## Description courte (affichée dans les résultats de recherche)

Le contrôle de l'éclairage, en parlant directement à l'appareil.

## Description

Candeo pilote l'éclairage de votre appareil en parlant au matériel lui-même. Aucun logiciel constructeur, aucun service en arrière-plan, aucun compte : l'application écrit par l'interface HID que Windows expose déjà, et les effets que vous écrivez s'exécutent à l'intérieur — ils continuent donc de tourner fenêtre fermée.

Compatible avec le Razer DeathStalker V2 Pro (filaire) ; un appareil pris en charge est nécessaire. Chaque appareil piloté par Candeo vient d'un protocole relevé sur le matériel et vérifié écriture par écriture. La liste est sur https://o2csi.github.io/candeo/devices.html.

CE QU'IL FAIT
• Toute la matrice : couleurs unies, dégradés rangée par rangée, et les effets que le firmware de l'appareil exécute lui-même.
• Des effets écrits en TypeScript, exécutés dans l'application : une onde qui suit votre frappe, un spectre, ce que vous écrivez.
• Des automatisations : des règles qui prennent la main un moment, puis vous rendent votre effet — l'heure à chaque heure pile, le clavier éteint la nuit ou pendant votre absence. Écrites comme une phrase, ou en expression cron.
• Un effet Clock fait défiler l'heure sur le clavier, et les effets que vous écrivez peuvent lire l'heure eux aussi.
• Les effets continuent fenêtre fermée — Candeo se range dans la zone de notification et l'éclairage reste.
• Vos effets sont des fichiers ordinaires, dans Documents\candeo\effects : modifiez-les ici, ou dans l'éditeur dont vous avez l'habitude.
• Démarrage à l'ouverture de session si vous le demandez. Thème clair et sombre. Français et anglais.

CE QU'IL NE FAIT PAS
• Il ne téléphone à personne. Rien n'est collecté, rien n'est envoyé : cette version du Store ne fait aucune requête réseau, et les mises à jour viennent du Store.
• Il ne s'impose pas. Quittez-le : l'appareil garde l'éclairage qu'il avait.

Le protocole qu'il parle a été établi en observant le matériel, et il est documenté publiquement, avec la version de firmware sur laquelle chaque fait a été vérifié. Le code source est disponible sous licence GPL-3.0.

## Nouveautés de cette version (0.7.0)

• Les automatisations, dans leur propre onglet : une règle interrompt l'effet d'un appareil un moment, puis le lui rend. Toutes les heures, en semaine, à une heure donnée — ou n'importe quelle expression cron, dans Avancé.
• Une règle peut aussi attendre votre absence : après 10 minutes sans touche ni souris, afficher Off, jusqu'au retour de quelqu'un.
• Essayer lance une règle une fois, tout de suite. Reprendre, sur l'appareil et dans la zone de notification, vous rend votre effet aussitôt. Mettre en pause les automatisations les suspend toutes, le temps d'une réunion ou d'une partie.
• Clock, un nouvel effet : l'heure défile sur le clavier, avec les secondes si vous le souhaitez.
• Les effets peuvent lire l'heure : écrivez votre propre cadran en TypeScript.
• Les paramètres d'un effet apparaissent dans l'ordre où son auteur les a écrits.

## Termes de recherche (7 au maximum, 30 caractères chacun)

hid, rgb, éclairage clavier, razer, deathstalker, effets typescript, open source
