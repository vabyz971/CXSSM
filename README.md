# CXSSM — CarX Street Sound Mod

Éditeur de sons pour **CarX Street** (PC), écrit en Rust avec l'interface [GPUI](https://gpui.rs).

CXSSM ouvre le fichier `fmod_assets_all.bundle` du jeu, détecte les banques **FMOD** qu'il
contient (régime moteur, turbo, échappement, musiques, voix, interface…) et permet de
remplacer chaque son par un fichier audio de mêmes caractéristiques. Il écrit ensuite un nouveau
`fmod_assets_all.bundle` **dans un autre dossier**, prêt à être publié sur le Steam Workshop.
Le fichier d'origine n'est jamais modifié.

> ⚠ Projet en cours de développement. La lecture et la réécriture ont été validées sur le
> vrai `fmod_assets_all.bundle` (62 banques, environ 3 000 sons, réécriture identique à l'octet près),
> mais un son remplacé n'a pas encore été testé **en jeu**. Voir [docs/RECHERCHE_FMOD.md](docs/RECHERCHE_FMOD.md).

---

## Fonctionnement

```
fmod_assets_all.bundle (UnityFS)
 └─ SerializedFile "CAB-…"
     └─ TextAsset  "Vehicles.bank" …       ← une banque FMOD Studio par TextAsset
         └─ RIFF "FEV " (.bank)
             ├─ évènements / paramètres     ← conservés tels quels (RPM, charge, bus…)
             └─ chunk "SND " → FSB5         ← les sons, remplacés un par un
```

1. **Ouvrir** : décompression du bundle (LZ4/LZMA), lecture des TextAssets, puis
   analyse des banques et du FSB5 (nom, codec, fréquence, canaux, durée, boucle de chaque son).
2. **Remplacer** : vous choisissez un WAV, OGG, FLAC ou MP3. CXSSM vérifie la fréquence et le
   nombre de canaux par rapport à l'original (indicateur vert ou rouge).
3. **Exporter** : ré-encodage dans le codec d'origine, reconstruction du FSB5, de la banque,
   du SerializedFile puis du bundle, écrit dans `<dossier choisi>/fmod_assets_all.bundle`
   avec un récapitulatif `cxssm-mod.txt`.

## Préparer un son de remplacement

| Règle | Détail |
|---|---|
| **Même fréquence** | ex. 48000 Hz si l'original est en 48000 Hz. Sinon la hauteur change. |
| **Même nombre de canaux** | les sons moteur sont généralement en mono. |
| **Durée proche** | surtout pour les **boucles moteur** : boucle parfaite, sans clic, sans fondu. |
| **Même hauteur au même régime** | une boucle `*_on_3000` doit être enregistrée à environ 3000 tr/min. |
| **WAV 16 bits** conseillé | c'est la source la plus fiable pour le ré-encodage. |

Les sons moteur vont par paires *on* (sous charge) et *off* (décélération) pour
chaque plage de régime. Remplacez toujours une **série complète** (toutes les plages
d'un même moteur), sinon le mélange entre vos sons et ceux d'origine s'entend.

Pour préparer les fichiers avec [Audacity](https://www.audacityteam.org/) :
« Projet → Fréquence », « Pistes → Convertir en mono », puis « Exporter en WAV 16 bits ».

## Prérequis

- **Rust** 1.85+ (`rustup`)
- **FMOD Engine** (gratuit, compte requis : <https://www.fmod.com/download>). Il fournit
  l'encodeur officiel, indispensable car **tous les sons de CarX Street sont en Vorbis FMOD**
  (ce n'est pas un Ogg standard). Indiquez son chemin :
  ```sh
  # Linux : le binaire s'appelle `fsbank` (dossier bin/ de l'archive fmodstudioapi…linux)
  export CXSSM_FSBANKCL=~/Downloads/fmodstudioapi20400linux/bin/fsbank
  # Windows : setx CXSSM_FSBANKCL "C:\Program Files (x86)\FMOD SoundSystem\FMOD Studio API Windows\bin\fsbankcl.exe"
  ```
  La variable doit être définie **dans le terminal qui lance l'application**.
- Linux : bibliothèques de GPUI (Vulkan, Wayland/X11, xkbcommon, fontconfig).
- **NixOS** : `shell.nix` fournit tout (Rust 1.95, cargo, clippy, rustfmt, rust-analyzer et les
  bibliothèques). Aucune installation de rustup n'est nécessaire :
  ```sh
  nix-shell --run 'cargo run -p cxssm-app --release'
  ```

## Compiler et lancer

```sh
cargo run -p cxssm-app --release [-- <bundle>]   # interface graphique (ouvre directement le bundle si fourni)
cargo run -p cxssm-cli -- list  <chemin>/fmod_assets_all.bundle
cargo run -p cxssm-cli -- bundle <chemin>/fmod_assets_all.bundle
cargo run -p cxssm-cli -- replace <bundle> --bank V8 --sound sample_0003 --file mon_son.wav --out output/
cargo test                                # tests des crates (le crate GPUI est exclu par défaut)
CXSSM_TEST_BUNDLE=<bundle> cargo test -p cxssm-core -- --ignored   # tests sur le vrai fichier du jeu
```

## Publier sur le Steam Workshop

CarX Street prend en charge le Workshop officiellement depuis la version **1.14.0**.

1. Exportez votre mod avec CXSSM dans un dossier dédié, par exemple `MonModMoteur/`.
2. Ouvrez **CarX Street Workshop Uploader** (Bibliothèque Steam → Outils).
3. Créez un nouvel élément et sélectionnez le dossier. Le bundle doit garder le nom
   `fmod_assets_all.bundle` et la même arborescence relative que dans le jeu
   (⚠ arborescence exacte à confirmer avec l'uploader).
4. Ajoutez une description. Le contenu de `cxssm-mod.txt` liste les sons modifiés.
5. Dans le jeu, allez dans **Paramètres → Mods**, puis abonnez-vous au mod et activez-le.
   Si deux mods modifient le même fichier, c'est celui qui a la priorité la plus haute qui s'applique.

> Ne remplacez pas le bundle directement dans le dossier du jeu : des fichiers modifiés de cette
> façon peuvent bloquer la connexion en ligne. Passez par le Workshop.

## Architecture du projet

```
CXSSM/
├── Cargo.toml              workspace : dépendances et lints communs
├── shell.nix               environnement de dev NixOS
├── crates/
│   ├── cxssm-unity/        UnityFS (lecture LZ4/LZMA, écriture LZ4), SerializedFile, TextAsset
│   ├── cxssm-fmod/         banque RIFF/FEV, FSB5 (lecture, réécriture, remplacement d'un échantillon)
│   ├── cxssm-core/         logique métier : ModProject, catégories, vérification audio, encodeur, export
│   ├── cxssm-cli/          binaire `cxssm` (tests sur le vrai fichier, scripts)
│   └── cxssm-app/          binaire GPUI : theme.rs, widgets.rs, view.rs
└── docs/
    ├── RECHERCHE_FMOD.md   formats, contraintes, risques, plan de validation
    └── mockup/index.html   maquette de l'interface (ouvrir dans un navigateur)
```

Principes :
- **Couches séparées** : les formats binaires (`unity`, `fmod`) ne connaissent ni l'UI ni l'audio,
  et `core` ne connaît pas GPUI. La CLI et l'application partagent la même logique.
- Erreurs typées avec `thiserror` dans les bibliothèques, `anyhow` dans les binaires.
- `unsafe_code = "forbid"` et clippy activé sur tout le workspace.
- Les tâches lourdes (ouverture, export) s'exécutent sur l'exécuteur d'arrière-plan de GPUI,
  pour ne pas bloquer l'interface.

## Feuille de route

- [x] Lecture et écriture UnityFS, SerializedFile, TextAsset
- [x] Analyse des banques FMOD et du FSB5, réécriture du FSB5
- [x] Encodage PCM16 natif, Vorbis/FADPCM via `fsbankcl`
- [x] Interface GPUI : catégories, liste, détails, remplacement, export
- [x] Validation de la lecture et de la réécriture sur le vrai `fmod_assets_all.bundle`
- [ ] **Test en jeu d'un son remplacé**
- [x] Écoute des sons d'origine et enregistrement en WAV (décodage FMOD Vorbis natif)
- [ ] Recherche, glisser-déposer, liste des banques dans la barre latérale
- [ ] Rééchantillonnage et conversion mono/stéréo automatiques
- [ ] Réglage du chemin `fsbankcl` et de la qualité dans l'interface
- [ ] Sauvegarde et réouverture d'un projet de mod (`.cxssm`)
- [ ] Déchiffrement des banques chiffrées, si c'est le cas pour CarX Street

## Avertissement

Projet de fans, sans lien avec CarX Technologies ni Firelight Technologies (FMOD).
Ne distribuez que vos propres sons, ou des sons dont vous avez les droits.

## Crédits

La table des en-têtes Vorbis FMOD vient de [fsbex](https://github.com/astral4/fsbex)
(MIT/Apache-2.0). Voir [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

## Licence

MIT — voir [LICENSE](LICENSE).
