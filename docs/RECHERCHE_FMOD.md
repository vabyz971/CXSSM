# Recherche : sons FMOD de CarX Street — extraction et recompilation

> Document de travail. Les points marqués **⚠ à vérifier** doivent être confirmés
> sur le vrai `fmod_assets_all.bundle` (commande `cxssm bundle` / `cxssm list`).

## 1. Chaîne des formats

```
fmod_assets_all.bundle          AssetBundle Unity, format "UnityFS"
└── CAB-xxxxxxxx                SerializedFile (table d'objets Unity)
    └── TextAsset (class 49)    un par banque : m_Name = "Vehicles.bank", m_Script = octets bruts
        └── .bank               banque FMOD Studio : RIFF, forme "FEV "
            ├── chunks LIST/…   évènements, paramètres (RPM, charge…), bus, GUIDs → À NE PAS TOUCHER
            └── chunk "SND "    contient un FSB5
                └── FSB5        en-têtes d'échantillons + noms + données audio
```

### 1.1 UnityFS
- En-tête big-endian : `"UnityFS"`, version (6–8), version Unity, taille, BlocksInfo.
- Données découpées en blocs compressés **LZ4/LZ4HC** (le plus courant) ou **LZMA**.
- Implémenté dans `crates/cxssm-unity/src/bundle.rs` (lecture LZ4/LZMA, réécriture LZ4).

### 1.2 FMOD for Unity et les banques
- Le plugin FMOD for Unity peut charger les banques depuis des **AssetBundles** :
  chaque `.bank` est alors un `TextAsset` dont `m_Script` contient les octets bruts
  ([doc FMOD for Unity](https://fmod.com/docs/2.03/unity/user-guide.html)).
- Le nom `fmod_assets_all.bundle` correspond à ce mode de chargement.
- `Master.bank` et `Master.strings.bank` contiennent le mixage global et les noms
  des évènements : il faut les laisser intacts.

### 1.3 FSB5
- En-tête de 0x3C octets (v1) : nombre d'échantillons, tailles des tables, **codec global**.
- Chaque échantillon : fréquence, canaux, nombre de samples PCM, offset des données,
  et des chunks optionnels (boucle, CRC de l'en-tête Vorbis…).
- Format décrit à partir de vgmstream (`fsb5.c`) et de
  [python-fsb5](https://github.com/HearthSim/python-fsb5).
- Codecs usuels sur PC : **Vorbis** (le plus probable pour la musique et les voix),
  **FADPCM** (souvent utilisé pour les boucles moteur, peu coûteux à décoder) et **PCM16**.

## 2. Pourquoi « mêmes caractéristiques que l'original »

Les évènements FMOD Studio (ex. l'évènement moteur piloté par le paramètre `RPM`)
référencent les sons **par index dans le FSB**, avec des réglages calés sur la
durée et la boucle d'origine. Un son de remplacement doit donc avoir :

| Caractéristique        | Pourquoi                                                          |
|------------------------|-------------------------------------------------------------------|
| même **codec**         | le codec est global au FSB                                        |
| même **fréquence**     | sinon la hauteur et la vitesse changent (moteur désaccordé)       |
| même **nombre de canaux** | les bus et le panoramique sont configurés pour ce nombre      |
| **durée proche**       | les boucles moteur sont des boucles parfaites ; les points de boucle sont recalculés |
| **même nom / index**   | c'est ainsi que la banque retrouve le son                         |

Pour les **boucles moteur** (une boucle par plage de régime, *on load* et *off load*),
le son doit boucler sans clic et garder la même hauteur que l'original au même régime,
sinon le crossfade entre les plages s'entend.

## 3. Ré-encodage : le cas Vorbis

FMOD ne stocke pas les en-têtes Vorbis standard : il les remplace par un **CRC32** qui
pointe vers une table d'en-têtes pré-calculés, intégrée au moteur. Un fichier `.ogg`
ordinaire ne peut donc pas être injecté tel quel. Il y a deux options :

1. **`fsbankcl` (recommandé)** : outil en ligne de commande officiel, fourni avec le
   *FMOD Engine* ([fmod.com/download](https://www.fmod.com/download), compte gratuit).
   CXSSM l'appelle pour chaque son, puis transplante l'échantillon produit dans le FSB d'origine.
   ```
   fsbankcl -format vorbis -quality 80 -o out.fsb mon_son.wav     (⚠ syntaxe exacte à vérifier)
   ```
2. **PCM16** : pas d'encodeur externe nécessaire. CXSSM écrit les données lui-même
   (`crates/cxssm-core/src/encoder.rs`).

## 4. Outils existants (référence et vérification)

| Outil | Usage |
|---|---|
| [AssetRipper / UABEA / UnityPy](https://github.com/K0lb3/UnityPy) | ouvrir le bundle et vérifier les TextAssets |
| [vgmstream](https://vgmstream.org) | écouter et convertir des FSB5 et `.bank` en WAV (vérifier un export) |
| [python-fsb5](https://github.com/HearthSim/python-fsb5), [fsbex (Rust)](https://docs.rs/fsbex) | extraction d'échantillons FSB5 |
| [Fmod5Sharp](https://github.com/Hengle/Fmod5Sharp) | décodage FSB5 (C#) |
| `fsbext` (rebuild) | reconstruction de FSB. Les retours d'utilisateurs confirment la contrainte « même taille, débit et fréquence » |

## 5. Risques connus

- **Contrôle d'intégrité / connexion en ligne** : un moddeur indique sur Nexus que des
  bundles reconstruits l'ont empêché de se connecter au jeu
  ([Nexus mod 36](https://www.nexusmods.com/carxstreet/mods/36)). La voie sûre est de
  **distribuer via le Steam Workshop** (support officiel depuis la 1.14.0), qui
  remplace les fichiers de façon autorisée, plutôt que d'écraser le bundle du jeu.
- **Banques chiffrées** : FMOD Studio permet de chiffrer les banques avec une clé. Si
  `cxssm list` signale « banque illisible (chiffrée ?) », la clé doit être récupérée et
  CXSSM devra déchiffrer le chunk SND (pas encore implémenté).
- **Hash dans l'en-tête FSB5** : il est conservé à l'identique. ⚠ Il faut vérifier que
  FMOD ne le revalide pas après modification.
- **Champs `byte_start` des objets Unity** : ils sont réécrits avec un alignement de 8 octets.
  ⚠ Le test réel n'a pas encore été fait sur le fichier du jeu.

## 6. Plan de validation sur le vrai fichier

1. `cxssm bundle fmod_assets_all.bundle` : version UnityFS, compression, liste des nœuds.
2. `cxssm list fmod_assets_all.bundle` : banques, codec, sons.
3. **Aller-retour** : `Fsb5::parse` puis `to_bytes` sur chaque banque, et comparaison
   octet par octet avec l'original. Toute différence (padding de la table des noms,
   ordre des chunks…) doit être comprise avant de tester en jeu. Le bundle, lui, est
   recompressé en LZ4 : il ne sera pas identique octet par octet, mais il doit se relire
   avec les mêmes banques.
4. Remplacer un son court (UI), tester en jeu, puis une boucle moteur.
5. Publier en privé sur le Workshop, puis tester l'abonnement et l'activation.
