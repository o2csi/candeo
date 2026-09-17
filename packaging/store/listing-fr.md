# Store listing — French (fr-FR)

The French listing for the Microsoft Store submission (#126), the translation of
`listing-en.md`. It uses the words the French interface uses: *piloter*,
*appareil*, *effets*, *paramètres*.

Version described: **0.5.1**.

## Description courte (affichée dans les résultats de recherche)

Le contrôle de l'éclairage, en parlant directement à l'appareil.

## Description

Candeo pilote l'éclairage de votre appareil en parlant au matériel lui-même. Aucun logiciel constructeur, aucun service en arrière-plan, aucun compte : l'application écrit par l'interface HID que Windows expose déjà, et les effets que vous écrivez s'exécutent à l'intérieur — ils continuent donc de tourner fenêtre fermée.

Compatible avec le Razer DeathStalker V2 Pro (filaire) ; un appareil pris en charge est nécessaire. Chaque appareil piloté par Candeo vient d'un protocole relevé sur le matériel et vérifié écriture par écriture. La liste est sur https://o2csi.github.io/candeo/devices.html.

CE QU'IL FAIT
• Toute la matrice : couleurs unies, dégradés rangée par rangée, et les effets que le firmware de l'appareil exécute lui-même.
• Des effets écrits en TypeScript, exécutés dans l'application : une onde qui suit votre frappe, un spectre, ce que vous écrivez.
• Les effets continuent fenêtre fermée — Candeo se range dans la zone de notification et l'éclairage reste.
• Vos effets sont des fichiers ordinaires, dans Documents\candeo\effects : modifiez-les ici, ou dans l'éditeur dont vous avez l'habitude.
• Démarrage à l'ouverture de session si vous le demandez. Thème clair et sombre. Français et anglais.

CE QU'IL NE FAIT PAS
• Il ne téléphone à personne. Rien n'est collecté, rien n'est envoyé : cette version du Store ne fait aucune requête réseau, et les mises à jour viennent du Store.
• Il ne s'impose pas. Quittez-le : l'appareil garde l'éclairage qu'il avait.

Le protocole qu'il parle a été établi en observant le matériel, et il est documenté publiquement, avec la version de firmware sur laquelle chaque fait a été vérifié. Le code source est disponible sous licence GPL-3.0.

## Nouveautés de cette version (0.5.1)

• L'application a sa propre icône, et le programme d'installation la porte aussi.
• Elle peut démarrer à l'ouverture de votre session, par la tâche de démarrage de Windows.
• Les paramètres indiquent la version que vous utilisez, et que cette version du Store est mise à jour par le Store.
• Les textes trop pâles pour être lus ont été corrigés partout, dans les deux thèmes.
• Les paramètres affichent le dossier des journaux avec ~ à la place du nom de votre compte.

## Termes de recherche (7 au maximum, 30 caractères chacun)

hid, rgb, éclairage clavier, razer, deathstalker, effets typescript, open source
