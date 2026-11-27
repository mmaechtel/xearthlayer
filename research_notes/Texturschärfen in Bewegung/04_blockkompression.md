# Blockkompression (BC1/DXT1), JPEG-Artefakte und Schärfen; texturfeste Artefakte in Bewegung

> **ACCESS LIMITATION (read first).** In this session `WebFetch` could not open any page:
> `fgiesen.wordpress.com`, `www.ludicon.com` and `www.intel.com` all failed with DNS errors
> because they are outside the session's allowed network domains; per the tool's instruction
> no further hosts were tried. **No primary source was opened and read in full.** Everything
> under "Cited Findings" below comes from **search-engine result summaries/snippets** of the
> linked pages (16 tool calls, 13 searches). Treat each item as "snippet-level, not verified
> against full text". Measured numbers from the papers' tables (PSNR/RMSE) could **not** be
> extracted and are listed as gaps, not invented. To verify, the hosts above (plus
> `mrelusive.com`, `developer.download.nvidia.com`, `tsapps.nist.gov`, `userpages.cs.umbc.edu`)
> must be allowed in the session's network settings.
>
> Labels: MEASURED / THEORY / VENDOR CLAIM / OPINION as requested; "(snippet)" = seen only as
> search summary.

## 1. BC1 error characteristics: which content is hard, and does higher local contrast increase BC1 error?

### Takeaway
The accessible sources agree on the mechanisms (4 colours on one line per 4x4 block, RGB565 endpoints, ~±3% decoder tolerance) and name smooth gradients and multi-colour blocks as weak points, but I found **no study that measures BC1 error or block visibility as a function of sharpening / local contrast**. That sharpening raises absolute BC1 error is a well-founded inference, not a cited measurement.

### Cited Findings
- Format (THEORY/definition, snippet): BC1 stores each 4x4 block as two 5:6:5 endpoint colours plus a 2-bit index table; 64 bits per block, 6:1. — [Wikipedia, "S3 Texture Compression"](https://en.wikipedia.org/wiki/S3_Texture_Compression)
- Hard content: gradients (OPINION/reference text, snippet): "due to the use of 5:6:5 colors, BC1 cannot represent smooth gradients well"; a second source states that for blocks with only two colours the palette is not the problem, i.e. banding stems from endpoint quantisation. — [N. Reed, "Understanding BCn Texture Compression Formats" (blog)](https://www.reedbeta.com/blog/understanding-bcn-texture-compression-formats/); [C. Buranicz, "Block Compression" (blog)](https://www.connburanicz.com/block-compression-bc1-bc7) (the search summary did not say which of the two pages carries which sentence)
- Hard content: slow two-colour transitions (snippet): DXT1 "usually gives quite good image quality for regular textures"; the most important failure case named is "slow transitions between two colors". — source among the search hits for S3TC background, most likely [Wikipedia S3TC](https://en.wikipedia.org/wiki/S3_Texture_Compression) (attribution uncertain)
- Practitioner observation (OPINION, forum, snippet): "Any sort of gradients or colour changes will come out looking blocky" with DXT; **subtle noise in the texture can slightly mask it**. — [Polycount thread "Normal map compression(?) artifacts"](https://polycount.com/discussion/74219/normal-map-compression-artifacts)
- Decoder tolerance (OPINION/blog by an expert, snippet): F. Giesen, "GPU BCn decoding" (The ryg blog, 2021): the spec allows a relative error of about ±3%, and decoders differ enough that "we don't even get an accurate picture of what the error from encoding any particular texture in BC1-5 is". — [Giesen 2021](https://fgiesen.wordpress.com/2021/10/04/gpu-bcn-decoding/) (page itself not reachable)
- Encoder-side detail (MEASURED in paper, snippet): J.M.P. van Waveren, "Real-Time DXT Compression", Intel/id Software, 20 May 2006: reports RMS error and throughput for DXT1/DXT5 on the 256x256 Lena image; insetting the colour bounding box by 1/16 "slightly improves the quality because it lowers the overall RMS error". Exact RMSE table values were garbled in the search extract. — [van Waveren 2006 (PDF)](https://mrelusive.com/publications/papers/Real-Time-Dxt-Compression.pdf)
- Colour blocking as a named DXT1 artefact (VENDOR/author claim, snippet): van Waveren & Castaño, "Real-Time YCoCg-DXT Compression" (id Software / NVIDIA, 2007): the YCoCg-DXT5 approach has "no color blocking artifacts" and "very little color shift due to 5:6 quantization" — implying both are recognised artefacts of plain DXT1. — [van Waveren & Castaño 2007 (PDF)](https://developer.download.nvidia.com/whitepapers/2007/Real-Time-YCoCg-DXT-Compression/Real-Time%20YCoCg-DXT%20Compression.pdf)
- Encoder quality ranking (OPINION/author claims, snippet): I. Castaño, "BC1 Compression Revisited" (blog, 2022): NVIDIA Texture Tools "had the highest quality openly available BC1 encoder"; contrasts the iterative strategy of `stb_dxt.h` with his cluster-fit approach (icbc). Later post "Spark 1.4" (2026): gains from 3-colour mode for near-black texels, a new endpoint quantisation procedure and greedy endpoint refinement. — [Castaño 2022](https://www.ludicon.com/castano/blog/2022/11/bc1-compression-revisited/); [Castaño, "Spark 1.4"](https://www.ludicon.com/castano/blog/2026/09/spark-1-4/)
- Endpoint rounding matters (OPINION/README, snippet): R. Geldreich's bc7enc/rgbcx BC1/BC3 encoder "also implements Castano's optimal endpoint rounding improvement". — [richgel999/bc7enc_rdo_devel](https://github.com/richgel999/bc7enc_rdo_devel/)
- Perceptual evaluation in rendered context (MEASURED, abstract-level snippet): W. Griffin & M. Olano, "Objective image quality assessment of texture compression", I3D 2014, pp. 119-126, DOI 10.1145/2556700.2556711: texture compression is normally judged per texture with MSE/PSNR/inspection, which the authors call the wrong approach; rendering a scene from sampled viewpoints with compressed vs. uncompressed textures shows that **masking has a significant effect on final rendered image quality** and that "GPU compression algorithms are too conservative and reduced bit rates are possible while maintaining quality". — [NIST record](https://www.nist.gov/publications/objective-image-quality-assessment-texture-compression); [author PDF](https://userpages.cs.umbc.edu/olano/papers/texiqa.pdf)
- Journal version (MEASURED, abstract-level snippet): Griffin & Olano, "Evaluating Texture Compression Masking Effects Using Objective Image Quality Assessment Metrics", IEEE TVCG 21(8):970-979, 2015, DOI 10.1109/TVCG.2015.2429576: masking effects and sensitivity vary by texture type; geometric masking affects **diffuse** texture artefacts more than normal-map artefacts (sec. 4.3.3). — [PubMed](https://pubmed.ncbi.nlm.nih.gov/26357259/); [NIST PDF](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=917324)

### Inferences
- (THEORY, own derivation, not from a source.) A BC1 block offers 4 colours evenly spaced on the segment between the endpoints. Along the line the quantisation step is (endpoint distance)/3, so the in-line error scales linearly with the block's colour range; the off-line residual (colours not on the line) also scales linearly when local contrast is multiplied. An unsharp mask that multiplies high-frequency amplitude by a factor k therefore raises absolute BC1 error in detailed blocks by roughly k (RMSE up, PSNR against the sharpened reference down by about 20·log10 k dB), while the ratio detail-to-error stays roughly constant. Sharpening does not make BC1 relatively worse per block; it makes the absolute error larger.
- (THEORY.) USM overshoot adds a third/fourth "colour" (halo dark/bright) to edge blocks; with coloured edges the halo can lie off the block's colour line, which is exactly the case BC1 handles worst (more than two dominant colours, off-line distribution).
- (THEORY.) Sharpening does not worsen RGB565 gradient banding in flat areas (USM leaves flat areas unchanged) unless it amplifies noise/JPEG ripple there; amplified fine noise may even mask banding (consistent with the Polycount observation), at the cost of higher measured error.
- Orthophoto content is "diffuse texture with natural noise", the class where Griffin & Olano found masking strongest; this argues that BC1 error increase from moderate sharpening is unlikely to be the dominant visible effect, but that is an extrapolation.

### Gaps
- No source found that measures BC1 PSNR/SSIM or block visibility versus sharpening strength or local variance. **No solid study exists as far as this search could establish.**
- Not accessed (so no numbers): Iourcha et al. S3TC patent; Lysenko, Intel "Fast ISPC Texture Compressor" (intel.com blocked); Nystad et al. 2012 ASTC; Ström & Akenine-Möller (PACKMAN/ETC); Olano et al. 2011 "Variable bit rate GPU texture decompression"; Guo/Lavoué et al. 2016. Their PSNR tables are not reported here.
- ISPC BC1 kernel specifics (PCA + endpoint refinement, PSNR vs. other encoders) could not be confirmed; the only ISPC number surfaced concerned BC6H in a third-party README and is irrelevant.
- Exact figures from Griffin & Olano (bit rates, MS-SSIM values) not obtained.

## 2. JPEG + sharpening (8x8 blocking, ringing) and the aligned 8x8 / 4x4 grids

### Takeaway
That sharpening amplifies JPEG blocking, ringing and noise is stated consistently in papers and patents and motivates "artifact-aware" sharpening, but I obtained **no number quantifying how much USM raises a blockiness metric**. The grid-alignment consequences are my own reasoning; I found no source on them.

### Cited Findings
- Sharpening amplifies JPEG artefacts (peer-reviewed, snippet): Bhattacharya et al., "Nonlinear kernel based feature maps for blur-sensitive unsharp masking of JPEG images", IET Image Processing, 2023: proposes an "artifact map" so that enhancement skips regions with ringing and blockiness instead of boosting them; notes that blur-based unsharp masking and JPEG artefact removal have mostly been studied separately. — [IET Image Processing 2023](https://ietresearch.onlinelibrary.wiley.com/doi/full/10.1049/ipr2.12692)
- (Patent text = VENDOR CLAIM, snippet): sharpening "often brings out compression artifacts and noise" and creates halos/ringing. — [US 9,142,009 "Patch-based, locally content-adaptive image and video sharpening"](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/9142009)
- (Patent, snippet): USM ringing = light/dark outline at strong edges, not prevented by the basic technique. — [US 6,804,408](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6804408)
- (Paper, snippet): raising the enhancement amount manually makes ringing and noise worse. — [IAENG Int. J. Computer Science 38(3)](https://www.iaeng.org/IJCS/issues_v38/issue_3/IJCS_38_3_03.pdf)
- Standard remedy order (patent, snippet): separate deblocking and deringing filters applied **before** enhancement. — [US 7,079,703 "JPEG artifact removal"](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7079703)
- Konstantinides, Bhaskaran & Beretta, "Image sharpening in the JPEG domain", IEEE TIP 8(6):874-878, 1999, DOI 10.1109/83.766864 (abstract-level snippet): sharpening done by scaling the entries of the quantisation table to boost high frequencies; reported for scanned images, "improved text and image quality with no additional computation cost and without affecting compressibility". (Addresses sharpening at encode time, not sharpening of an already decoded low-quality JPEG.) — [catalogue record](https://katalog.dhi-paris.fr/vufind/Record/NLM177548436); related patent [US 5,850,484](https://patents.google.com/patent/US5850484)
- Kim & Allebach, "Optimal unsharp mask for image sharpening and noise removal", J. Electronic Imaging 14(2):023005, 2005, DOI 10.1117/1.1924510 (abstract-level snippet): USM strength made locally adaptive by training, smoothing flat areas and sharpening detailed ones; reported superior to fixed-strength USM. — [SPIE](https://electronicimaging.spiedigitallibrary.org/journals/journal-of-electronic-imaging/volume-14/issue-02/023005/Optimal-unsharp-mask-for-image-sharpening-and-noise-removal/10.1117/1.1924510.full)

### Inferences
- (THEORY, own.) The literature's consistent answer to "sharpen a compressed image" is adaptive strength: no or negative gain in flat regions (where 8x8 steps and noise are visible), gain only on real detail, and deblock/dering first. A global fixed-gain USM on mip 0 is the variant the literature treats as the baseline to be improved.
- (THEORY, own.) Grid alignment in this pipeline: 256 px tiles in a 4096 px texture put every JPEG 8x8 boundary on a BC1 4x4 boundary (one JPEG block = 2x2 BC1 blocks; 4:2:0 chroma blocks span 16x16 px = 4x4 BC1 blocks). Consequences: (a) a JPEG block step never falls inside a BC1 block at mip 0, so BC1 does not spend its 4-colour palette on it — a small advantage; (b) both discontinuity sources stack at the same 8 px lines, and a USM kernel straddling such a line amplifies the step there; (c) at mip 1 the JPEG grid has period 4 px = BC1 grid, at mip 2 it has period 2 px and lies inside BC1 blocks, but box filtering has by then averaged most of the step away.
- (THEORY, own.) 4:2:0 chroma is smooth over 2 px, so chroma varies slowly inside a 4x4 BC1 block; sharpening luminance only (not RGB independently) keeps the block's colours closer to a single line through colour space (luma axis), which is BC1's best case.

### Gaps
- No measured figure for the increase of a blockiness metric under USM was obtained (Zhai et al. and comparable deblocking/blockiness papers not accessed).
- Full texts of Konstantinides 1999 and Kim & Allebach 2005 not read; only abstracts via search.
- No source discusses coincident JPEG and BC1 block grids.

## 3. Double compression: JPEG -> (sharpen) -> BC1

### Takeaway
The problem is acknowledged in the literature in passing ("artefacts of both formats"), but I found **no quantitative study** of BC1 quality on JPEG-decoded input, with or without intermediate sharpening.

### Cited Findings
- (Paper, snippet): a paper on lossless recompression of already compressed textures notes that a texture that went through JPEG and then a texture codec carries artefacts of both, and that fast JPEG-to-texture recompression takes shortcuts affecting quality. — [Ström & Wennersten(?), "Lossless Compression of Already Compressed Textures" (ResearchGate record; authorship not verified)](https://www.researchgate.net/publication/221249071_Lossless_Compression_of_Already_Compressed_Textures)
- (Survey, snippet): idTech 5 virtual texturing transcodes JPEG-like data to DXT at run time; result "contains artifacts from both formats". — ["Texture Compression Techniques", Scientific Visualization journal, 2014](http://sv-journal.org/2014-1/06/en/index.php?lang=en)
- Related practice (same family of work): van Waveren's real-time DXT papers were written for exactly this transcode-at-runtime use. — [van Waveren 2006](https://mrelusive.com/publications/papers/Real-Time-Dxt-Compression.pdf)
- (Wiki, OPINION, snippet): re-encoding DXT after editing compounds the loss; work from source files. — [Valve Developer Community, "Restoring Texture After DXT Compression"](https://developer.valvesoftware.com/wiki/Restoring_Texture_After_DXT_Compression)

### Inferences
- (THEORY, own.) JPEG at q75-90 has already removed much high-frequency chroma and some luma texture; the input to BC1 is therefore smoother than the original and easier for BC1. Sharpening reverses part of that smoothing but restores amplitude at frequencies where the remaining signal is partly quantisation noise, so the added BC1 error is spent partly on encoding amplified JPEG noise.

### Gaps
- No study found; this must be measured locally (e.g. PSNR/SSIM/FLIP of BC1(decoded JPEG) and BC1(USM(decoded JPEG)) against the respective uncompressed input, per mip level).

## 4. Motion: visibility of texture-locked artefacts; mip transitions; moiré

### Takeaway
I found **no study on the visibility of texture-space block-compression artefacts in motion**, nor on BC block patterns across trilinear mip transitions or block-grid moiré. The video literature documents the opposite case (screen-fixed error, "dirty window") as conspicuous, and vision science says sensitivity to high spatial frequencies falls with retinal velocity; both suggest content-locked fine artefacts are comparatively benign, but that is theory by analogy.

### Cited Findings
- "Dirty window" effect (trade press, OPINION/description, snippet): noise granules appear to stay put while objects move underneath; attributed to insufficient residual bits in predicted frames so the error persists until the next I-frame. — [TV Technology, "Compression technology"](https://www.tvtechnology.com/news/compression-technology)
- (Peer-reviewed, snippet): Rehman & Evans, "Alleviating Dirty-Window Effect in Medium Frame-Rate Binary Video Halftones", IEEE TIP (PubMed 22155962): the effect is a temporal artefact arising when the binary pattern does not change sufficiently with the underlying content; commonly observed at medium frame rates. — [PubMed](https://pubmed.ncbi.nlm.nih.gov/22155962/); framework paper: [EURASIP J. Image and Video Processing 2010](https://jivp-eurasipjournals.springeropen.com/articles/10.1155/2010/625191)
- (Patent, VENDOR CLAIM, snippet): in stereo video, coding errors "float in the air"; awareness of the effect makes viewers notice it. — [EP 2556677 A1](https://patents.google.com/patent/EP2556677A1/en)
- Spatio-velocity CSF (peer-reviewed/standard model, snippet): Kelly's spatiotemporal CSF, measured with stabilised retinal images, is a function of spatial frequency and **retinal** velocity; "as retinal velocity increases the sensitivity to high spatial frequencies is lost first while the sensitivity to low spatial frequencies is relatively preserved"; at very low retinal velocities low-frequency sensitivity is even increased. — [Yee, Pattanaik & Greenberg, "Spatiotemporal sensitivity and visual attention for efficient rendering of dynamic environments" (arXiv cs/0511032)](https://arxiv.org/pdf/cs/0511032); [Aleph-map manuscript copy](https://ics.uci.edu/~majumder/vispercep/alephmap.pdf)
- Role of pursuit (same sources, snippet): eye tracking "reduces the retinal velocity of the tracked objects and thus compensates for the loss of sensitivity due to motion"; tracking is imperfect for fast/erratic motion, leaving residual retinal motion that reduces resolvability. — [arXiv cs/0511032](https://arxiv.org/pdf/cs/0511032)
- Modern CSF reference (peer-reviewed, title-level only): Ashraf et al., "castleCSF — A contrast sensitivity function of color, area, spatiotemporal frequency, luminance and eccentricity", Journal of Vision 24(4):5, 2024. — [JoV](https://jov.arvojournals.org/article.aspx?articleid=2793530)
- Trilinear filtering purpose (encyclopaedic, snippet): blends the two nearest mip levels to hide the abrupt quality change at level boundaries; mipmapping reduces but does not eliminate shimmering under minification. — [Wikipedia, "Texture filtering"](https://en.wikipedia.org/wiki/Texture_filtering)
- (Preprint, snippet): trilinear filtering itself causes block- and diamond-shaped artefacts absent with tricubic filtering. — [Pharr et al., "Filtering After Shading With Stochastic Texture Filtering", arXiv 2407.06107](https://arxiv.org/pdf/2407.06107)

### Inferences
- (THEORY, own.) BC1 and JPEG errors are part of the texture: they translate rigidly with the ground. When the eye pursues the ground they are retinally near-stationary and as visible as in a still image, subject to pursuit gain; when the eye fixates the horizon/cockpit they sweep across the retina and, being high-spatial-frequency (4 px and 8 px periods), fall into the range the spatio-velocity CSF attenuates first. They cannot produce a dirty-window impression, which requires screen-fixed error.
- (THEORY, own.) What does change over time for a texture-locked error is its **resampling**: (1) per-level independent BC1 encoding means the error pattern of level n and n+1 are uncorrelated; in the trilinear blend zone the two patterns cross-fade as distance changes. Because blending averages two uncorrelated errors, amplitude in the zone is lower than in either level, not higher; a visible "swim" would require errors large enough to be seen in a still frame. (2) Under minification the sampled level has texels of about 0.5-1 screen pixel along the minor axis, so a 4-texel block grid has a period of roughly 2-4+ screen pixels at mip 0 only where mip 0 is actually selected; mip selection and anisotropic footprints keep it at or below the level where it can alias, unless LOD bias is negative. Block-grid moiré is therefore not expected with correct LOD; sharpened content raises aliasing energy in general (subject of the other researchers' notes), independent of BC1.
- (THEORY, own.) Sharpening only mip 0 creates a step in contrast between mip 0 and mip 1; in the trilinear zone and along anisotropic footprints this appears as a band where apparent sharpness changes with distance. This band is screen-space-located (at a fixed LOD distance) while the ground moves through it — the one mechanism in this pipeline that resembles a screen-fixed pattern.

### Gaps
- No psychophysical study found comparing visibility of content-locked vs. screen-locked compression artefacts under pursuit; none on VR mura/screen-door in relation to texture artefacts was retrieved.
- No study or practitioner write-up found on BC block patterns across mip levels in trilinear blending, or on block-grid moiré. Stated explicitly: **theory only**.

## 5. Bilinear magnification of BC1 blocks at close range

### Takeaway
No source retrieved addresses block-boundary visibility under magnification as a function of local contrast; only general statements that DXT gradients look blocky.

### Cited Findings
- (Forum, OPINION, snippet): gradients/colour changes come out blocky under DXT; noise slightly masks it. — [Polycount](https://polycount.com/discussion/74219/normal-map-compression-artifacts)
- (Blog/reference, snippet): BC1 cannot represent smooth gradients well because of 5:6:5 endpoints. — [Reed, "Understanding BCn Texture Compression Formats"](https://www.reedbeta.com/blog/understanding-bcn-texture-compression-formats/)
- Decoder interpolation differs between GPUs within about ±3% (expert blog, snippet). — [Giesen 2021](https://fgiesen.wordpress.com/2021/10/04/gpu-bcn-decoding/)

### Inferences
- (THEORY, own.) Under bilinear magnification the 4x4 structure becomes visible where adjacent blocks chose different endpoint pairs for similar content (palette discontinuity at the block edge). The discontinuity size is bounded by the quantisation step, which grows with in-block range; higher local contrast from sharpening therefore enlarges block-to-block palette mismatches in textured areas, while the same higher contrast masks them. Net visibility is undetermined without a test; smooth areas (water, fields, roofs) are where blocks show, and USM changes those least. At 2.4 m/texel, magnification occurs only at very low altitude where the texture is in any case far below display resolution.

### Gaps
- No measurement or study; must be evaluated by inspection in the simulator.

## 6. Industry guidance: sharpening before block compression; perceptual BC1 weighting

### Takeaway
Tool documentation discusses sharpening in the context of **mip generation filters** (Kaiser, sharpen passes), not as a pre-pass for block compression; I found no vendor statement for or against sharpening before BC1, and could not retrieve documentation on perceptual error weights.

### Cited Findings
- NVIDIA Texture Tools API docs (VENDOR documentation, snippet): Kaiser = windowed-sinc downsampling filter; defaults width 3.0, alpha 4.0, stretch 1.0; docs warn that it can ring or be less sharp than wanted and advise against widths above 5. BC1 is the default compression format. — [castano/nvidia-texture-tools wiki, ApiDocumentation](https://github.com/castano/nvidia-texture-tools/wiki/ApiDocumentation)
- Unity manual (VENDOR documentation, snippet): Kaiser mip filter described as "a sharpening algorithm" run on the mips as they get smaller. — [Unity Manual 2017.4, Texture Types](https://docs.unity3d.com/2017.4/Documentation/Manual/TextureTypes.html)
- (OPINION, blog, snippet): NVTT Kaiser gave little gain over box; a separate sharpen pass on mips helped. — [Pope Kim, "Mipmap Quality", 2013](https://www.popekim.com/2013/06/mipmap-quality.html)
- (OPINION, blog, snippet): B. Golus: NVIDIA Kaiser makes thin lines blurrier overall; Unity 2020.1.3 Kaiser shows ringing; possible Unity behaviour of deriving each mip from the previous one stacks sharpening. — [Golus, "Sharper Mipmapping using Shader Based Supersampling"](https://bgolus.medium.com/sharper-mipmapping-using-shader-based-supersampling-ed7aadb47bec)
- Tool with per-mip sharpen filters (documentation, title-level only): VTFLib/VTFEdit user guide to mip-map and sharpen filters. — [wunderboy.org](https://www.wunderboy.org/documentation/a-user-guide-to-the-vtf-mip-map-and-sharpen-filters-of-vtflib-vtfedit/)
- Griffin & Olano (MEASURED, abstract-level): GPU compression is "too conservative" once masking in the rendered image is considered. — [NIST record](https://www.nist.gov/publications/objective-image-quality-assessment-texture-compression)

### Inferences
- Industry practice sharpens **lower mips** (to counter box/Kaiser blur), i.e. the opposite end of the chain from the user's proposal (mip 0 only). No retrieved guidance treats mip 0 sharpening as standard practice or warns against it with respect to BC1.
- (THEORY, own.) Encoders with perceptual channel weighting (known from NVTT/stb/Compressonator options, not confirmed in this session) weight luma error higher; they do not model block-edge visibility or contrast masking, so they offer no built-in mitigation for sharpening-induced error. The Intel ISPC BC1 kernel used here exposes no perceptual weighting to my knowledge — unverified.

### Gaps
- Unreal Engine (MipGenSettings "Sharpen0-10"), AMD Compressonator and NVTT perceptual-weight documentation were not retrieved; not cited.
- No GDC talk on sharpening before block compression was found.
