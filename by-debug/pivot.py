import re
from collections import defaultdict

who = '''
Name
Martynka
Betty
Angela Farley 
Lara Wright 
Clemence
Lilly Whale
Jess
Rachael 
Jane
Olivia 
Jasmine 
Hayley 
Ruth 
Cara White
Lois Bennett
Marin 
Jenny
Shona 
Lian 
Laurie Fraser-Chalk
Kate
Alicia
Milly
Chelsea 
Aila 
Cicely Moag
Adriana
Keli W
Katherine Davies
Katie Myles 
Anna 
Daniela Rotbande
Molly
'''

when = '''
When are you available?
Wednesday morning, Wednesday afternoon
Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Thursday morning, Thursday afternoon, Friday afternoon
Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Monday afternoon, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday afternoon, Friday morning, Friday afternoon
Monday morning, Monday afternoon, Wednesday morning, Wednesday afternoon
Monday morning, Monday afternoon, Tuesday morning, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Friday afternoon
Tuesday afternoon, Wednesday morning, Wednesday afternoon, Friday morning, Friday afternoon
Monday afternoon, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday morning, Thursday afternoon, Friday afternoon
Monday morning, Monday afternoon, Tuesday afternoon, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Tuesday morning
Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday morning, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon
Monday morning, Tuesday morning, Tuesday afternoon, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon

Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday morning, Thursday afternoon
Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Monday afternoon, Friday morning, Friday afternoon
Monday afternoon, Tuesday afternoon, Wednesday afternoon, Thursday afternoon, Friday afternoon
Monday morning, Wednesday morning
Wednesday morning, Wednesday afternoon, Friday morning, Friday afternoon
Monday morning, Monday afternoon, Wednesday afternoon, Thursday afternoon, Friday afternoon
Monday morning, Monday afternoon
Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday afternoon, Thursday afternoon, Friday morning, Friday afternoon
Monday morning, Tuesday morning, Wednesday afternoon, Thursday morning, Friday morning
Monday morning, Monday afternoon, Tuesday morning, Tuesday afternoon, Wednesday morning, Wednesday afternoon, Thursday morning, Thursday afternoon, Friday morning, Friday afternoon
Monday morning, Tuesday morning, Thursday morning, Friday morning
Tuesday morning, Tuesday afternoon, Friday morning, Friday afternoon
'''

what = '''
What are you after (you can choose multiple answers)?
Exercising, you want to feel like you are being active!, A “normal walk”
A “normal walk”
Exercising, you want to feel like you are being active!, A “normal walk”, I'm looking to meet up with a few lovely people and enjoy a walk or a nice cuppa somewhere during the week whilst my children are at school. I work nights shifts as a carer so I may not be able to meet up on a certain day this week but could do that day another week. Mornings would work best but could meet up lunchtimes but need to get back to cheriton for 3 pm each day.
A “normal walk”, Exercising, you want to feel like you are being active!, 
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc)., A “normal walk”
Exercising, you want to feel like you are being active!, A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc)., A “normal walk”
Exercising, you want to feel like you are being active!
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc)., A “normal walk”, Exercising, you want to feel like you are being active!
A “normal walk”, Exercising, you want to feel like you are being active!
Exercising, you want to feel like you are being active!
A “normal walk”
Exercising, you want to feel like you are being active!, A “normal walk”
A “normal walk”, Exercising, you want to feel like you are being active!, A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc).
A “normal walk”, Exercising, you want to feel like you are being active!, A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc).
Exercising, you want to feel like you are being active!, A “normal walk”
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc).
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc).
Exercising, you want to feel like you are being active!, A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc)., A “normal walk”
Exercising, you want to feel like you are being active!, A “normal walk”
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc)., I’m not available yet until I start my mat leave!
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc)., Exercising, you want to feel like you are being active!
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc)., Exercising, you want to feel like you are being active!, A “normal walk”
Exercising, you want to feel like you are being active!, Too far to do some body weight exercises as well? 
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc).
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc)., Exercising, you want to feel like you are being active!
Exercising, you want to feel like you are being active!, A “normal walk”
A “normal walk”, Happy to meet new ladies with similar age kids to go to the beach / parks / local events when my child is not at school. Especially at weekends we stay out and walk from park to park 
A “normal walk”
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc).
A “normal walk”, Exercising, you want to feel like you are being active!
A gentle walk (you gave birth not long ago/your toddler will walk too/you just want to get out/etc).
Exercising, you want to feel like you are being active!, A “normal walk”
Exercising, you want to feel like you are being active!, A “normal walk”
'''


def main():
    whos = [s.strip() for s in who.strip().split('\n')[1:]]
    whens = when.strip().split('\n')[1:]
    whats = what.strip().split('\n')[1:]
    whens = [w.split(', ') for w in whens]
    whats = [re.split(', (?=[A-Z])', w) for w in whats]
    scores = defaultdict(list)
    for (w, es, ts) in zip(whos, whens, whats):
        for e in es:
            for t in ts:
                scores[(e, t)].append(w)

    for (e, t), ws in list(sorted(scores.items(), key=lambda x: len(x[1]), reverse=True))[:10]:
        print(f'{len(ws)}\t{e}\t{t[:14]}\t{', '.join(sorted(ws))}')

    by_when = defaultdict(list)
    for (w, es) in zip(whos, whens):
        for e in es:
            by_when[e].append(w)

    for e, ws in sorted(by_when.items(), key=lambda x: len(x[1]), reverse=True):
        print(f'{len(ws)}\t\t{e}\t{', '.join(sorted(ws))}')


    by_what = defaultdict(list)
    for (w, ts) in zip(whos, whats):
        for t in ts:
            by_what[t].append(w)

    for t, ws in sorted(by_what.items(), key=lambda x: len(x[1]), reverse=True):
        print(f'{len(ws)}\t\t{t[:14]}\t{', '.join(sorted(ws))}')


if __name__ == '__main__':
    main()
