# img-bed

A self-hosted, single-owner image host: the Owner uploads images and gets public links to paste into Markdown, HTML, or chat. Replaces a retired Lsky Pro instance.

## Language

**Owner**:
The single person who operates the instance, logs into the admin area, and is the only one allowed to upload.
_Avoid_: User, admin account, member, uploader

**API Token**:
A named, revocable secret the Owner creates so an external tool (PicGo, ShareX, a script) can upload on the Owner's behalf.
_Avoid_: API key, access key, password

**Visitor**:
Anyone who views an image through its public link without logging in. Visitors can never upload.
_Avoid_: Guest, anonymous user

**Image**:
One uploaded file, kept byte-for-byte as uploaded, plus its record. An Image is identified by its content: identical bytes are always the same Image. Images imported from Lsky Pro are ordinary Images, not a separate kind.
_Avoid_: Picture, photo, file, upload

**Duplicate Upload**:
An upload whose bytes match an existing Image. Nothing new is created; the Owner is told the Image already exists and is shown its Paths.
_Avoid_: Re-upload, copy

**Path**:
The part of a public link after the domain, pointing at exactly one Image. An Image has one or more Paths, all of which serve it equally; the Owner can add and remove Paths at any time. A Path the Owner removes is immediately free to be given to any Image, including a different one (Paths of a trashed Image are not removed; see Trash).
_Avoid_: URL (the URL is domain + Path), alias, key, filename, slug

**Reserved Prefix**:
A leading part of the address space that can never be a Path because the admin area and API live there.
_Avoid_: Route, system path

**Path Template**:
The Owner's setting that generates the first Path of each new Image. Changing it affects only future uploads, never existing Paths.
_Avoid_: Naming rule, naming strategy, pattern

**Preset**:
A ready-made Path Template the Owner can pick instead of writing one.

**Primary Path**:
The one Path of an Image that the admin area displays and copies by default. Any of the Image's Paths can be made Primary.
_Avoid_: Canonical URL, main link

**Rename**:
Informal shorthand for adding a new Path to an Image and removing an old one; not a separate operation.

**Trash**:
Where a deleted Image waits for 30 days before it is removed for good. A trashed Image is not served to Visitors but still holds all its Paths, so none of them can be given to another Image.
_Avoid_: Recycle bin, archive, soft delete

**Restore**:
Bringing an Image back from the Trash with all its Paths intact. A Duplicate Upload of a trashed Image restores it.

**Permanent Delete**:
Removing an Image from the Trash (by the Owner, or automatically after 30 days); its file is erased and its Paths become free.
_Avoid_: Purge, hard delete

**Import**:
The one-time transfer of Images from the retired Lsky Pro instance, keeping each Image's original Lsky Pro link as one of its Paths. Done once by hand (or by an agent), not a feature of the product.
_Avoid_: Migration (reserved for database schema changes), sync
