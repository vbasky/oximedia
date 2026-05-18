# Bibliography

Citations are numbered. The text refers to them as `[1]`, `[2]`,
etc. Entries are grouped by category for readability, then ordered
alphabetically within each group. Within each entry, the format is:

> Author(s). *Title*. Venue, year. **[short tag used in text]**

No URLs — they go stale. Title + author + year + venue is enough to
find anything in this list with a single search.

---

## Foundational textbooks

1. Cover, T. M., and Thomas, J. A. *Elements of Information Theory*,
   2nd ed. Wiley-Interscience, 2006. **[CoverThomas2006]**
   — the canonical information-theory text. Chapters 2 (entropy), 3
   (asymptotic equipartition), 10 (rate-distortion) are the relevant
   ones for codec engineers.

2. Sayood, K. *Introduction to Data Compression*, 5th ed. Morgan
   Kaufmann, 2017. **[Sayood2017]**
   — the friendliest treatment of entropy coding, transforms, and
   quantization in isolation, with worked examples. The chapter on
   arithmetic coding is the clearest exposition in print.

3. Poynton, C. *Digital Video and HD: Algorithms and Interfaces*,
   2nd ed. Morgan Kaufmann, 2012. **[Poynton2012]**
   — color science, gamma, YCbCr derivation, signal-level video
   theory. The book you give to a new hire who's making color bugs.

4. Strang, G., and Nguyen, T. *Wavelets and Filter Banks*. Wellesley-
   Cambridge Press, 1996. **[StrangNguyen1996]**
   — the math of transforms, multirate signal processing, and
   filterbanks. Useful even for DCT-only codecs because the DCT is a
   filterbank.

5. Vetterli, M., and Kovačević, J. *Wavelets and Subband Coding*.
   Prentice Hall, 1995. **[VetterliKovacevic1995]**
   — out of print but freely available from the authors; the
   compression-oriented complement to Strang/Nguyen.

## Codec-specific textbooks

6. Richardson, I. E. *The H.264 Advanced Video Compression Standard*,
   2nd ed. Wiley, 2010. **[Richardson2010]**
   — the most readable book-length H.264 reference.

7. Sze, V., Budagavi, M., and Sullivan, G. J. (eds). *High Efficiency
   Video Coding (HEVC): Algorithms and Architectures*. Springer, 2014.
   **[SzeBudagaviSullivan2014]**
   — the equivalent for HEVC, written by people who built it.

8. Wien, M. *High Efficiency Video Coding: Coding Tools and
   Specification*. Springer, 2015. **[Wien2015]**
   — alternative HEVC reference, more compact.

9. Bosi, M., and Goldberg, R. E. *Introduction to Digital Audio Coding
   and Standards*. Springer, 2003. **[BosiGoldberg2003]**
   — the AAC bible. Psychoacoustic models, MDCT, the lot.

10. Hartmann, P. *MXF: The Material Exchange Format*. Focal Press,
    2010. **[Hartmann2010]**
    — for anyone working in broadcast: the canonical reference on MXF.

## Seminal papers

11. Shannon, C. E. "A Mathematical Theory of Communication." *Bell
    System Technical Journal*, 27(3, 4), 1948, pp. 379–423 and
    623–656. **[Shannon1948]**
    — the founding paper of the field. Read at least the first ten
    pages once in your life.

12. Sullivan, G. J., and Wiegand, T. "Rate-distortion optimization for
    video compression." *IEEE Signal Processing Magazine*, 15(6),
    November 1998, pp. 74–90. **[SullivanWiegand1998]**
    — establishes J = D + λR as the optimization framework, and
    derives the empirical λ ≈ c·Q² relationship that every encoder
    still uses.

13. Bjøntegaard, G. "Calculation of average PSNR differences between
    RD-curves." ITU-T SG16 Q.6 Video Coding Experts Group, document
    VCEG-M33, Austin, TX, April 2001. **[Bjontegaard2001]**
    — the document that defines BD-rate. Two pages. Read it.

14. Wiegand, T., Sullivan, G. J., Bjøntegaard, G., and Luthra, A.
    "Overview of the H.264/AVC Video Coding Standard." *IEEE
    Transactions on Circuits and Systems for Video Technology*,
    13(7), July 2003, pp. 560–576. **[Wiegand2003]**
    — the H.264 overview paper. Required reading.

15. Sullivan, G. J., Ohm, J.-R., Han, W.-J., and Wiegand, T.
    "Overview of the High Efficiency Video Coding (HEVC) Standard."
    *IEEE Transactions on Circuits and Systems for Video Technology*,
    22(12), December 2012, pp. 1649–1668. **[Sullivan2012]**
    — the HEVC overview paper.

16. Bross, B., Wang, Y.-K., Ye, Y., Liu, S., Chen, J., Sullivan, G. J.,
    and Ohm, J.-R. "Overview of the Versatile Video Coding (VVC)
    Standard and its Applications." *IEEE Transactions on Circuits
    and Systems for Video Technology*, 31(10), October 2021, pp.
    3736–3764. **[Bross2021]**
    — the VVC / H.266 overview paper.

17. Chen, Y., Murherjee, D., Han, J., Grange, A., Xu, Y., Liu, Z.,
    Parker, S., Chen, C., Su, H., Joshi, U., Chiang, C.-H., Wang, Y.,
    Wilkins, P., Bankoski, J., Trudeau, L., Egge, N., Valin, J.-M.,
    Davies, T., Midtskogen, S., Norkin, A., and de Rivaz, P. "An
    Overview of Coding Tools in AV1: The First Video Codec from the
    Alliance for Open Media." *APSIPA Transactions on Signal and
    Information Processing*, 9, e6, 2020. **[Chen2020]**
    — the AV1 overview paper. Open access.

18. Marpe, D., Schwarz, H., and Wiegand, T. "Context-Based Adaptive
    Binary Arithmetic Coding in the H.264/AVC Video Compression
    Standard." *IEEE TCSVT*, 13(7), July 2003, pp. 620–636.
    **[Marpe2003]**
    — the CABAC paper. The clearest description of CABAC outside the
    standard itself.

19. List, P., Joch, A., Lainema, J., Bjøntegaard, G., and Karczewicz,
    M. "Adaptive Deblocking Filter." *IEEE TCSVT*, 13(7), July 2003,
    pp. 614–619. **[List2003]**
    — the H.264 deblocking filter paper.

20. Tudor, P. N. "MPEG-2 Video Compression." *Electronics &
    Communication Engineering Journal*, 7(6), December 1995, pp.
    257–264. **[Tudor1995]**
    — a clear, short MPEG-2 overview that has aged remarkably well.

21. Wallace, G. K. "The JPEG Still Picture Compression Standard."
    *Communications of the ACM*, 34(4), April 1991, pp. 30–44.
    **[Wallace1991]**
    — the JPEG paper. The DCT-quantize-zigzag-entropy structure
    described here is still recognizable in every modern codec.

22. Ohm, J.-R., Sullivan, G. J., Schwarz, H., Tan, T. K., and Wiegand,
    T. "Comparison of the Coding Efficiency of Video Coding Standards
    — Including High Efficiency Video Coding (HEVC)." *IEEE TCSVT*,
    22(12), December 2012, pp. 1669–1684. **[Ohm2012]**
    — the canonical apples-to-apples comparison of MPEG-2, H.263,
    MPEG-4 Part 2, H.264, and HEVC.

23. Daede, T., Norkin, A., and Brailovskiy, I. "Video Codec Testing
    and Quality Measurement." IETF Internet-Draft
    draft-ietf-netvc-testing, 2020. **[Daede2020]**
    — the netvc working group's recommendations on how to compare
    codecs fairly.

## Standards documents (downloadable free from ITU / ISO / AOM)

24. ITU-T Recommendation H.264 / ISO/IEC 14496-10. *Advanced video
    coding for generic audiovisual services.* Current revision: 2021
    or later. **[H264Spec]**

25. ITU-T Recommendation H.265 / ISO/IEC 23008-2. *High efficiency
    video coding.* Current revision: 2023 or later. **[H265Spec]**

26. ITU-T Recommendation H.266 / ISO/IEC 23090-3. *Versatile video
    coding.* Current revision: 2022 or later. **[H266Spec]**

27. ITU-T Recommendation H.273 / ISO/IEC 23091-2. *Coding-independent
    code points for video signal type identification.* **[H273Spec]**
    — the canonical source for VUI / matrix coefficients / transfer
    characteristics. Read it once.

28. Alliance for Open Media. *AV1 Bitstream & Decoding Process
    Specification.* Version 1.0.0, errata 1, January 2019. **[AV1Spec]**

29. ISO/IEC 14496-12. *ISO base media file format.* The MP4 / MOV
    container spec. **[ISOBMFF]**

30. ISO/IEC 14496-3. *Information technology — Coding of audio-visual
    objects — Part 3: Audio.* The AAC spec. **[AACSpec]**

31. ITU-R Recommendation BT.709. *Parameter values for the HDTV
    standards for production and international programme exchange.*
    **[BT709]**

32. ITU-R Recommendation BT.2020. *Parameter values for ultra-high
    definition television systems.* **[BT2020]**

33. ITU-R Recommendation BT.2100. *Image parameter values for high
    dynamic range television.* **[BT2100]**

34. SMPTE ST 2084. *High Dynamic Range Electro-Optical Transfer
    Function of Mastering Reference Displays.* The PQ TF spec.
    **[ST2084]**

35. ARIB STD-B67. *Essential parameter values for the extended image
    dynamic range television.* The HLG TF spec. **[ARIBB67]**

36. SMPTE RDD 36. *Apple ProRes Bitstream Syntax and Decoding
    Process.* 2015. **[RDD36]**
    — the ProRes specification, freely downloadable from SMPTE.

37. RFC 6184. *RTP Payload Format for H.264 Video.* 2011.
    **[RFC6184]**

38. RFC 7798. *RTP Payload Format for High Efficiency Video Coding
    (HEVC).* 2016. **[RFC7798]**

39. RFC 6716. *Definition of the Opus Audio Codec.* 2012.
    **[RFC6716]**

40. RFC 8216. *HTTP Live Streaming.* 2017. **[RFC8216]**

41. ISO/IEC 23009-1. *Dynamic adaptive streaming over HTTP (DASH) —
    Part 1: Media presentation description and segment formats.*
    **[DASHSpec]**

## Open source codecs (reading code)

42. **FFmpeg / libavcodec**. The reference for every codec. Read the
    H.264 decoder in `libavcodec/h264*` first.

43. **x264**. The H.264 encoder. Exceptionally well-commented.
    `encoder/me.c`, `common/dct.c`, `encoder/cabac.c` are the canonical
    files to read for motion estimation, transforms, and entropy coding
    respectively.

44. **x265**. The HEVC encoder.

45. **libvpx**. The VP8/VP9 reference encoder/decoder.

46. **libaom**. The AV1 reference encoder/decoder.

47. **dav1d**. The AV1 decoder, optimized to within an inch of its
    life. The best modern reference for production SIMD work.

48. **SVT-AV1**. AV1 encoder by Intel/Netflix, designed for
    parallelism.

49. **rav1e**. AV1 encoder in Rust.

50. **JM (JVT reference software)**. The H.264 reference decoder.
    Slow, not production-grade, but bit-exact and easy to read.

51. **HM (HEVC reference software)**. The HEVC reference decoder.

## Lecture series and online courses

52. Girod, B. *EE368: Digital Image Processing* and *EE398A: Image
    and Video Compression*. Stanford University. **[GirodStanford]**
    — Bernd Girod is one of the field's foundational professors;
    Stanford has historically posted his slides and notes.

53. Wiegand, T. *Image and Video Compression* lecture series.
    Technical University of Berlin / Fraunhofer HHI. **[WiegandHHI]**
    — Wiegand's own lectures, mostly on YouTube under
    "Image-Communication-TUB" or similar channels.

54. Vetterli, M., and Goyal, V. *Foundations of Signal Processing*.
    Cambridge, 2014, with companion lectures on EPFL's open courseware.
    **[VetterliFSP]**

## Engineering blogs and writeups

55. **Daala demo posts** by Timothy B. Terriberry, Jean-Marc Valin,
    et al. The Xiph.org blog has a sequence of "demo" posts written
    while Daala was being developed; these became the AV1 design
    rationale and are the clearest written explanation of why AV1
    looks the way it does. **[XiphDaala]**

56. **Diary of an x264 developer**, Jason Garrett-Glaser (Dark
    Shikari). The x264 developer's blog, full of in-the-trenches
    insight on H.264 encoding. **[DarkShikari]**

57. **The Netflix Tech Blog**, particularly the VMAF posts and the
    per-shot rate control posts (Anne Aaron, Ioannis Katsavounidis,
    Zhi Li). **[NetflixTechBlog]**

58. **Doom9 forum archives**. Useful for arcane bitstream questions.
    **[Doom9]**

## Companion documents in this workspace

59. [`codec_internals.md`](../codec_internals.md) — generic
    block-coding pipeline, color, intra/inter prediction, transforms,
    quantization, entropy coding overview, loop filtering, rate
    control overview, profiles/levels, audio basics, container
    vocabulary. **[Internals]**

60. [`prores_decoder.md`](../prores_decoder.md) — the worked decoder
    example this book points at repeatedly. ProRes 422 from
    compressed bytes to 10-bit YUV samples, every stage worked.
    **[ProResWalkthrough]**

61. [`wire_formats.md`](../wire_formats.md) — H.264 NAL framing,
    RTP, SDP, HTTP Digest, NV12, CMTime, VideoToolbox. **[Wire]**

62. [`rate_control.md`](../rate_control.md) — production rate
    control, deeper than the textbook treatments. **[RateControl]**

63. [`simd_dispatch.md`](../simd_dispatch.md) — how SIMD dispatch
    works in this workspace. **[SIMDDispatch]**
