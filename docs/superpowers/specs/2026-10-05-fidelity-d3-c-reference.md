# NetHack 5.0 C Reference: Alignment Mechanics (Deliverable 3)

Exact citations and binding semantics extracted directly from vendored `NetHack-5.0.0/` source.

---

## 1. Initial Alignment Record (`urole.initrecord`)

### C Definition and Usage
- **Struct definition:** `include/you.h:222`
  ```c
  struct Role {
      ...
      xint16 xlev;               /* cutoff experience level */
      xint16 initrecord;         /* initial alignment record */
      ...
  };
  ```
- **Player initialization:** `src/attrib.c:1094`
  ```c
  u.ualign.record = gu.urole.initrecord;
  ```

### Values per Role in `src/role.c`
| Role | `xlev` | `initrecord` | Citation in `src/role.c` |
|---|---|---|---|
| Archeologist | 14 | **10** | Line 68 |
| Barbarian | 10 | **10** | Line 109 |
| Caveman | 10 | **0** | Line 150 |
| Healer | 20 | **10** | Line 190 |
| Knight | 10 | **10** | Line 230 |
| Monk | 10 | **10** | Line 271 |
| Priest | 10 | **0** | Line 312 |
| Rogue | 11 | **10** | Line 354 |
| Ranger | 12 | **10** | Line 409 |
| Samurai | 11 | **10** | Line 449 |
| Tourist | 14 | **0** | Line 489 |
| Valkyrie | 10 | **0** | Line 529 |
| Wizard | 12 | **0** | Line 570 |

---

## 2. Alignment Cap & Adjustment (`ALIGNLIM` & `adjalign`)

### C Definition in `include/align.h:17`
```c
/* bounds for "record" -- respect initial alignments of 10 */
#define ALIGNLIM (10L + (svm.moves / 200L))
```

### C Implementation in `src/attrib.c:1298-1316`
```c
/* avoid possible problems with alignment overflow, and provide a centralized
   location for any future alignment limits */
void
adjalign(int n)
{
    int newalign = u.ualign.record + n;

    if (n < 0) {
        unsigned newabuse = u.ualign.abuse - n;

        if (newalign < u.ualign.record)
            u.ualign.record = newalign;
        if (newabuse > u.ualign.abuse) {
            u.ualign.abuse = newabuse;
            adj_erinys(newabuse);
        }
    } else if (newalign > u.ualign.record) {
        u.ualign.record = newalign;
        if (u.ualign.record > ALIGNLIM)
            u.ualign.record = (int)ALIGNLIM;
    }
}
```

---

## 3. Monster Malign Calculation (`set_malign`)

### C Implementation in `src/makemon.c:2320-2366`
```c
/* Set malign to have the proper effect on player alignment if monster is
 * killed.  Negative numbers mean it's bad to kill this monster; positive
 * numbers mean it's good.  Since there are more hostile monsters than
 * peaceful monsters, the penalty for killing a peaceful monster should be
 * greater than the bonus for killing a hostile monster to maintain balance.
 * Rules:
 *   it's bad to kill peaceful monsters, potentially worse to kill always-
 *      peaceful monsters;
 *   it's never bad to kill a hostile monster, although it may not be good.
 */
void
set_malign(struct monst *mtmp)
{
    schar mal = mtmp->data->maligntyp;
    boolean coaligned;

    if (mtmp->ispriest || mtmp->isminion) {
        /* some monsters have individual alignments; check them */
        if (mtmp->ispriest && EPRI(mtmp))
            mal = EPRI(mtmp)->shralign;
        else if (mtmp->isminion && EMIN(mtmp))
            mal = EMIN(mtmp)->min_align;
        /* unless alignment is none, set mal to -5,0,5 */
        /* (see align.h for valid aligntyp values)     */
        if (mal != A_NONE)
            mal *= 5;
    }

    coaligned = (sgn(mal) == sgn(u.ualign.type));
    if (mtmp->data->msound == MS_LEADER) {
        mtmp->malign = -20;
    } else if (mal == A_NONE) {
        if (mtmp->mpeaceful)
            mtmp->malign = 0;
        else
            mtmp->malign = 20; /* really hostile */
    } else if (always_peaceful(mtmp->data)) {
        int absmal = abs(mal);
        if (mtmp->mpeaceful)
            mtmp->malign = -3 * max(5, absmal);
        else
            mtmp->malign = 3 * max(5, absmal); /* renegade */
    } else if (always_hostile(mtmp->data)) {
        int absmal = abs(mal);
        if (coaligned)
            mtmp->malign = 0;
        else
            mtmp->malign = max(5, absmal);
    } else if (coaligned) {
        int absmal = abs(mal);
        if (mtmp->mpeaceful)
            mtmp->malign = -3 * max(3, absmal);
        else /* renegade */
            mtmp->malign = max(3, absmal);
    } else /* not coaligned and therefore hostile */
        mtmp->malign = abs(mal);
}
```

---

## 4. Kill-Based Alignment Adjustments (`mon.c:3676-3726`)

### C Implementation in `src/mon.c:3676-3726`
```c
    /* adjust alignment points */
    if (mtmp->m_id == svq.quest_status.leader_m_id) { /* REAL BAD! */
        adjalign(-(u.ualign.record + (int) ALIGNLIM / 2));
        u.ugangr += 7; /* instantly become "extremely" angry */
        change_luck(-20);
        pline("That was %sa bad idea...",
              u.uevent.qcompleted ? "probably " : "");
        if (!svc.context.mon_moving)
            iter_mons(anger_quest_guardians);
    } else if (mdat->msound == MS_NEMESIS) { /* Real good! */
        if (!svq.quest_status.killed_leader)
            adjalign((int) (ALIGNLIM / 4));
    } else if (mdat->msound == MS_GUARDIAN) { /* Bad */
        adjalign(-(int) (ALIGNLIM / 8));
        u.ugangr++;
        change_luck(-4);
        if (!Hallucination)
            pline("That was probably a bad idea...");
        else
            pline("Whoopsie-daisy!");
    } else if (mtmp->ispriest) {
        adjalign((p_coaligned(mtmp)) ? -2 : 2);
        /* cancel divine protection for killing your priest */
        if (p_coaligned(mtmp))
            u.ublessed = 0;
        if (mdat->maligntyp == A_NONE)
            adjalign((int) (ALIGNLIM / 4)); /* BIG bonus */
    } else if (mtmp->mtame) {
        adjalign(-15); /* bad!! */
        /* your god is mighty displeased... */
        if (!Hallucination) {
            Soundeffect(se_distant_thunder, 40);
            You_hear("the rumble of distant thunder...");
        } else {
            Soundeffect(se_applause, 40);
            You_hear("the studio audience applaud!");
        }
        if (!unique_corpstat(mdat)) {
            boolean mname = has_mgivenname(mtmp);

            livelog_printf(LL_KILLEDPET, "murdered %s%s%s faithful %s",
                           mname ? MGIVENNAME(mtmp) : "",
                           mname ? ", " : "",
                           uhis(), pmname(mdat, Mgender(mtmp)));
        }
    } else if (mtmp->mpeaceful)
        adjalign(-5);

    /* malign was already adjusted for u.ualign.type and randomization */
    adjalign(mtmp->malign);
```
