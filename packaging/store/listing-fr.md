# Store listing — French (fr-FR)

The French listing for the Microsoft Store submission (#126), the translation of
`listing-en.md`. It uses the words the French interface uses: *piloter*,
*appareil*, *effets*, *paramètres*.

Version described: **0.8.0**. Its *What's new* covers everything since 0.7.0.

## Description courte (affichée dans les résultats de recherche)

Le contrôle de l'éclairage, en parlant directement à l'appareil.

## Description

Candeo pilote l'éclairage de votre appareil en parlant au matériel lui-même. Aucun logiciel constructeur, aucun service en arrière-plan, aucun compte : l'application écrit par l'interface HID que Windows expose déjà, et les effets que vous écrivez s'exécutent à l'intérieur — ils continuent donc de tourner fenêtre fermée.

Compatible avec le Razer DeathStalker V2 Pro (filaire), et avec le clavier et les zones lumineuses de l'Alienware m18 R1 ; un appareil pris en charge est nécessaire. Chaque appareil piloté par Candeo vient d'un protocole relevé sur le matériel et vérifié écriture par écriture. La liste est sur https://o2csi.github.io/candeo/devices.html.

CE QU'IL FAIT
• Toute la matrice : couleurs unies, dégradés rangée par rangée, et les effets que le micrologiciel de l'appareil exécute lui-même.
• Des effets écrits en TypeScript, exécutés dans l'application : une onde qui suit votre frappe, un spectre, ce que vous écrivez.
• Des automatisations : des règles qui prennent la main un moment, puis vous rendent votre effet — l'heure à chaque heure pile, le clavier éteint la nuit ou pendant votre absence. Écrites comme une phrase, ou en expression cron.
• Un effet Clock fait défiler l'heure sur le clavier, et les effets que vous écrivez peuvent lire l'heure eux aussi.
• Les effets continuent fenêtre fermée — Candeo se range dans la zone de notification et l'éclairage reste.
• Vos effets sont des fichiers ordinaires, dans Documents\candeo\effects : modifiez-les ici, ou dans l'éditeur dont vous avez l'habitude.
• Démarrage à l'ouverture de session si vous le demandez. Thème clair et sombre. Français et anglais.

CE QU'IL NE FAIT PAS
• Il ne téléphone à personne. Rien n'est collecté, rien n'est envoyé : cette version du Store ne fait aucune requête réseau, et les mises à jour viennent du Store.
• Il ne s'impose pas. Quittez-le : l'appareil garde l'éclairage qu'il avait.

Le protocole qu'il parle a été établi en observant le matériel, et il est documenté publiquement, avec la version de micrologiciel sur laquelle chaque fait a été vérifié. Le code source est disponible sous licence GPL-3.0.

## Nouveautés de cette version (0.8.0)

• Un deuxième appareil : l'Alienware m18 R1. Candeo pilote son clavier touche par touche, et l'anneau et le logo qui l'entourent en trois zones.
• Les sept effets du micrologiciel du m18 R1, sous les noms qu'Alienware leur donne, avec une couleur pour ceux qui en prennent une. Le simulateur les dessine aussi.
• Un paramètre modifié atteint aussitôt l'effet en cours, y compris les effets de l'appareil lui-même.
• Les effets qui réagissent aux touches ne sont plus proposés pour les zones lumineuses, où aucune touche n'est jamais pressée.
• Les messages d'erreur nomment l'appareil concerné, et chacun peut être sélectionné et fermé.

## Termes de recherche (7 au maximum, 30 caractères chacun)

alienware, rgb, éclairage clavier, razer, deathstalker, effets typescript, open source
